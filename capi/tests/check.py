#!/usr/bin/env python3
"""Validate the released Stage A API. Requires Python 3, Cargo, make, cc, c++, and nm.

--sanitize instruments both C callers with ASan/UBSan; Rust is not instrumented.
--rust-asan also instruments Rust with nightly (standard library is prebuilt).
Darwin Rust ASan requires non-Apple LLVM clang via CC/CXX and nightly llvm-tools.
Golden comparisons are claimed only for targets with released snapshots.
"""
import argparse
import difflib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CAPI = ROOT / "capi"
SPEC = ROOT / "docs/spec/c-api/stage-a"


def run(command, *, cwd=ROOT, env=None):
    result = subprocess.run([str(arg) for arg in command], cwd=cwd, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"{shlex.join(map(str, command))} exited {result.returncode}\n"
                           + result.stdout.decode(errors="replace")
                           + result.stderr.decode(errors="replace"))
    return result


def signature_source(functions):
    lines = ['#include <ucl.h>', 'int main(void) {']
    for index, function in enumerate(functions):
        name = function["name"]
        declaration, count = re.subn(r"\b" + re.escape(name) + r"\s*\(",
                                    f"(*signature_{index})(", function["declaration"], count=1)
        if count != 1:
            raise RuntimeError(f"Cannot parse released declaration: {function['declaration']}")
        lines.append("    " + declaration.rstrip().removesuffix(";") + f" = &{name};")
        lines.append(f"    if (signature_{index} == NULL) return 1;")
    lines += ['    return 0;', '}']
    return "\n".join(lines) + "\n"


def symbols(library, shared, nm):
    options = ["-gU"] if platform.system() == "Darwin" else (
        ["-D", "--defined-only"] if shared else ["-g", "--defined-only"])
    output = run([nm, *options, library]).stdout.decode()
    return {match.group(1) for match in re.finditer(r"\b_?(ucl_\w+)\s*$", output, re.MULTILINE)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--rust-asan", action="store_true",
                        help="Build Rust with nightly ASan and instrument C callers too")
    args = parser.parse_args()
    args.sanitize = args.sanitize or args.rust_asan
    functions = [entry for entry in json.loads((SPEC / "api.json").read_text())["functions"]
                 if entry["provided"]]
    if len(functions) != 43:
        raise RuntimeError(f"Expected 43 provided signatures, found {len(functions)}")
    if (CAPI / "include/ucl.h").read_bytes() != (SPEC / "include/ucl.h").read_bytes():
        raise RuntimeError("Shipping header differs from released Stage A header")
    cases = json.loads((SPEC / "cases.json").read_text())
    target = platform.system().lower() + "-" + platform.machine().lower()
    golden = ROOT / cases["golden"] / target
    compare_snapshots = golden.is_dir()
    if not compare_snapshots:
        print(f"No released oracle snapshots for {target}; comparisons skipped, "
              "no compatibility claim for this target")
    env = dict(os.environ, LC_ALL="C", TZ="UTC")
    # Keep the build location deterministic even if the caller sets CARGO_TARGET_DIR.
    env["CARGO_TARGET_DIR"] = str(CAPI / "target")
    if args.sanitize:
        # Darwin's ASan does not support LeakSanitizer.
        env["ASAN_OPTIONS"] = "detect_leaks=" + ("0" if platform.system() == "Darwin" else "1")
        env["UBSAN_OPTIONS"] = "halt_on_error=1:print_stacktrace=1"
    cargo = ["cargo"]
    build_options = ["--manifest-path", CAPI / "Cargo.toml", "--release"]
    release = CAPI / "target/release"
    if args.rust_asan:
        host = re.search(r"^host: (.+)$", run(["rustc", "+nightly", "-vV"]).stdout.decode(),
                         re.MULTILINE).group(1)
        cargo += ["+nightly"]
        env["CARGO_TARGET_DIR"] = str(CAPI / "target/asan")
        env["RUSTFLAGS"] = "-Zsanitizer=address"
        build_options += ["--target", host]
        release = CAPI / "target/asan" / host / "release"
    execution_env = env.copy()
    if args.sanitize and platform.system() == "Darwin":
        compiler = shlex.split(os.environ.get("CC", "cc"))
        resource = Path(run([*compiler, "-print-resource-dir"]).stdout.decode().strip())
        runtime = resource / "lib/darwin/libclang_rt.asan_osx_dynamic.dylib"
        if not runtime.is_file():
            raise RuntimeError(f"Missing compiler ASan runtime: {runtime}")
        # Ensure the sanitizer's interceptors load before an instrumented Rust dylib.
        execution_env["DYLD_INSERT_LIBRARIES"] = str(runtime)
        if args.rust_asan:
            env["RUSTFLAGS"] += f" -Zexternal-clangrt=yes -Clink-arg={runtime}"
    run([*cargo, "build", *build_options], env=env)
    native = run([*cargo, "rustc", *build_options,
                  "--", "--print=native-static-libs"], env=env)
    output = (native.stdout + native.stderr).decode()
    match = re.search(r"native-static-libs:\s*([^\n]+)", output)
    if not match:
        raise RuntimeError("Cargo did not report static native link dependencies")
    dependencies = shlex.split(match.group(1))
    shared = release / ("libucl.dylib" if platform.system() == "Darwin" else "libucl.so")
    # Rust's LLVM matches archive bitcode; Apple nm can be older than rustc's LLVM.
    rustc = ["rustc", "+nightly"] if args.rust_asan else ["rustc"]
    sysroot = Path(run([*rustc, "--print=sysroot"]).stdout.decode().strip())
    host = re.search(r"^host: (.+)$", run([*rustc, "-vV"]).stdout.decode(), re.MULTILINE).group(1)
    rust_nm = sysroot / "lib/rustlib" / host / "bin/llvm-nm"
    nm = os.environ.get("NM") or (str(rust_nm) if rust_nm.is_file() else shutil.which("llvm-nm") or "nm")
    expected = {entry["name"] for entry in functions}
    for library, is_shared in [(release / "libucl.a", False), (shared, True)]:
        exported = symbols(library, is_shared, nm)
        if exported != expected:
            raise RuntimeError(f"{library.name}: missing {sorted(expected - exported)}, "
                               f"unexpected {sorted(exported - expected)}")
    print("Static/shared exports match exactly the 43 provided symbols")
    with tempfile.TemporaryDirectory(prefix="c15-check-") as directory:
        work = Path(directory)
        signature = work / "signatures.c"
        signature.write_text(signature_source(functions))
        flags = ["-Wall", "-Wextra", "-Werror", "-pedantic", "-pthread", "-I", CAPI / "include"]
        sanitize = (["-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-g", "-O1"]
                    if args.sanitize else [])
        variants = [("static", release, CAPI / "include"),
                    ("shared", release, CAPI / "include")]
        if not args.rust_asan:
            prefix = work / "install"
            run(["make", "-C", CAPI, "install", f"PREFIX={prefix}"], env=env)
            if (prefix / "include/ucl.h").read_bytes() != (SPEC / "include/ucl.h").read_bytes():
                raise RuntimeError("Installed header differs from the released header")
            variants += [("installed-static", prefix / "lib", prefix / "include"),
                         ("installed-shared", prefix / "lib", prefix / "include")]
        for linkage, libdir, includedir in variants:
            link = ([libdir / "libucl.a", *dependencies] if linkage.endswith("static") else
                    [libdir / shared.name, f"-Wl,-rpath,{libdir}"])
            compile_flags = [*flags[:-2], "-I", includedir]
            for language, standard, compiler in [("c", "c11", os.environ.get("CC", "cc")),
                                                  ("c++", "c++11", os.environ.get("CXX", "c++"))]:
                binary = work / f"signatures-{linkage}-{language}"
                run([*shlex.split(compiler), "-x", language, f"-std={standard}", *compile_flags,
                     signature, "-x", "none", *sanitize, *link, "-o", binary], env=env)
                run([binary], env=execution_env)
            for label, source in [("probe", ROOT / cases["source"]),
                                  ("extra", CAPI / "tests/extra.c")]:
                binary = work / f"{label}-{linkage}"
                run([*shlex.split(os.environ.get("CC", "cc")), "-std=c11",
                     "-D_POSIX_C_SOURCE=200809L", *compile_flags, *sanitize,
                     source, *link, "-o", binary], env=env)
                if label == "extra":
                    run([binary], cwd=work, env=execution_env)
                    continue
                for case in cases["cases"]:
                    case_dir = work / f"{linkage}-{case}"
                    case_dir.mkdir()
                    result = run([binary, case], cwd=case_dir, env=execution_env)
                    if not compare_snapshots:
                        continue
                    expected_bytes = (golden / f"{case}.txt").read_bytes()
                    if result.stdout != expected_bytes:
                        difference = "".join(difflib.unified_diff(
                            expected_bytes.decode().splitlines(keepends=True),
                            result.stdout.decode().splitlines(keepends=True),
                            fromfile=f"golden/{case}", tofile=f"{linkage}/{case}"))
                        raise RuntimeError(difference)
            print(f"{linkage}: 43 signatures, C11/C++11 linkage, "
                  f"{len(cases['cases']) if compare_snapshots else 0} snapshots, "
                  "boundary/lifetime/depth passed")
    if args.sanitize:
        print("C callers passed ASan/UBSan; "
              + ("Rust passed nightly ASan; " if args.rust_asan else "Rust was not instrumented; ")
              + ("leak detection unavailable on Darwin" if platform.system() == "Darwin"
                 else "leak detection enabled"))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError) as error:
        raise SystemExit(str(error)) from error
