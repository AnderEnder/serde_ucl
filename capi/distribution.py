#!/usr/bin/env python3
"""Build and verify opt-in source/macOS arm64 C distributions (no publishing)."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CARGO_C = "0.10.25+cargo-0.99.0"
ABI = "1.0.0"
SPEC = "docs/spec/c-api/stage-a"


def run(args, *, cwd=ROOT, env=None):
    result = subprocess.run(list(map(str, args)), cwd=cwd, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"{shlex.join(list(map(str, args)))}\n"
                           + result.stdout.decode(errors="replace")
                           + result.stderr.decode(errors="replace"))
    return result.stdout.decode()


def environment():
    env = dict(os.environ, LC_ALL="C", TZ="UTC", MACOSX_DEPLOYMENT_TARGET="11.0")
    # Builds must not inherit a developer's target directory or Rust linker overrides.
    for name in ("CARGO_TARGET_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(name, None)
    return env


def require_tools():
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        raise RuntimeError("SDKs are released only for verified macOS arm64")
    if CARGO_C not in run(["cargo", "cinstall", "--version"]):
        raise RuntimeError(f"Install cargo-c {CARGO_C} with --locked")


def install(source, prefix):
    env = environment()
    env["CARGO_TARGET_DIR"] = str(source / "capi/target/distribution")
    run(["cargo", "cinstall", "--locked", "--release", "--manifest-path",
         source / "capi/Cargo.toml", "--target", "aarch64-apple-darwin",
         "--prefix", prefix, "--libdir", prefix / "lib"], cwd=source, env=env)


def relocate(prefix):
    # cargo-c correctly versions the dylib; the portable SDK uses an rpath ID.
    run(["install_name_tool", "-id", "@rpath/libucl.1.dylib", prefix / "lib/libucl.1.dylib"])
    pc = prefix / "lib/pkgconfig/serde-ucl.pc"
    lines = pc.read_text().splitlines()
    replacements = {"prefix": "${pcfiledir}/../..", "exec_prefix": "${prefix}",
                    "libdir": "${prefix}/lib", "includedir": "${prefix}/include"}
    pc.write_text("\n".join(next((f"{key}={value}" for key, value in replacements.items()
                                    if line.startswith(key + "=")), line)
                            for line in lines) + "\n")


def checksums(directory):
    files = sorted(p for p in directory.rglob("*") if p.is_file() and not p.is_symlink()
                   and p.name != "SHA256SUMS")
    (directory / "SHA256SUMS").write_text("".join(
        f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(directory)}\n" for p in files))


def third_party_licenses(source):
    """Carry dependency notices without vendoring their implementation sources."""
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1",
                               "--manifest-path", source / "capi/Cargo.toml"], cwd=source))
    notices = source / "licenses"
    for package in metadata["packages"]:
        if not package["source"]:
            continue
        crate = Path(package["manifest_path"]).parent
        destination = notices / "dependencies" / f"{package['name']}-{package['version']}"
        destination.mkdir(parents=True)
        found = False
        for item in crate.iterdir():
            if item.is_file() and any(word in item.name.lower() for word in ("license", "copying", "notice")):
                shutil.copy2(item, destination / item.name)
                found = True
        if not found:
            raise RuntimeError(f"Missing license notices for {package['name']}")
    rust_docs = Path(run(["rustc", "--print=sysroot"]).strip()) / "share/doc/rust"
    rust = notices / "rust"
    rust.mkdir()
    shutil.copy2(rust_docs / "COPYRIGHT-library.html", rust / "COPYRIGHT-library.html")
    shutil.copytree(rust_docs / "licenses", rust / "licenses")


def verify_checksums(directory):
    for line in (directory / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        assert hashlib.sha256((directory / name).read_bytes()).hexdigest() == digest, name


def archive(directory, output):
    # Stable ordering and ownership; preserve relative library symlinks.
    with tarfile.open(output, "w:gz", format=tarfile.PAX_FORMAT) as tar:
        def metadata(info):
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            return info
        tar.add(directory, arcname=directory.name, filter=metadata)


def source_tree(directory):
    files = ["Cargo.toml", "Cargo.lock", "README.md", "CHANGELOG.md", "LICENSE-MIT",
             "LICENSE-APACHE", "capi/Cargo.toml", "capi/Cargo.lock", "capi/README.md",
             "capi/Makefile", "capi/distribution.py", "capi/IMPLEMENTATION.md",
             "capi/tests/check.py", "capi/tests/extra.c", "capi/include/ucl.h",
             "docs/clean-room/PROTOCOL.md", "docs/clean-room/LOG.md"]
    for folder in ("src", "capi/src", "tests/conformance/capi/stage-a"):
        files += [str(p.relative_to(ROOT)) for p in (ROOT / folder).rglob("*") if p.is_file()]
    for name in files:
        destination = directory / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, destination)
    # Cargo validates explicitly named benches even for a path dependency. C source
    # releases omit the benchmark harness, so omit those manifest targets too.
    manifest = directory / "Cargo.toml"
    manifest.write_text(re.sub(r'\n\[\[bench\]\]\nname = "[^"]+"\nharness = false\n',
                               "\n", manifest.read_text()))
    # Only released spec files can enter the source artifact.
    names = run(["git", "ls-tree", "-r", "--name-only", "spec-v22", "docs/spec"]).splitlines()
    for name in names:
        data = subprocess.check_output(["git", "show", f"spec-v22:{name}"], cwd=ROOT)
        destination = directory / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)


def verify_library(prefix, source, *, portable):
    spec = source / SPEC
    assert (prefix / "include/ucl.h").read_bytes() == (spec / "include/ucl.h").read_bytes()
    assert (prefix / "lib/libucl.a").is_file()
    assert (prefix / "lib/libucl.dylib").is_symlink()
    identity = run(["otool", "-L", prefix / "lib/libucl.1.dylib"])
    expected = "@rpath/libucl.1.dylib" if portable else str(prefix / "lib/libucl.1.dylib")
    assert expected in identity, identity
    assert "compatibility version 1.0.0, current version 1.0.0" in identity, identity
    build = run(["vtool", "-show-build", prefix / "lib/libucl.1.dylib"])
    assert re.search(r"minos\s+11\.0", build), build
    assert "arm64" in run(["lipo", "-info", prefix / "lib/libucl.1.dylib"])
    spec_module = importlib.util.spec_from_file_location("capi_check", source / "capi/tests/check.py")
    check = importlib.util.module_from_spec(spec_module)
    spec_module.loader.exec_module(check)
    functions = [x for x in json.loads((spec / "api.json").read_text())["functions"] if x["provided"]]
    expected_symbols = {x["name"] for x in functions}
    assert len(expected_symbols) == 43
    sysroot = Path(run(["rustc", "--print=sysroot"]).strip())
    nm = sysroot / "lib/rustlib/aarch64-apple-darwin/bin/llvm-nm"
    for library, shared in [(prefix / "lib/libucl.a", False), (prefix / "lib/libucl.1.dylib", True)]:
        assert check.symbols(library, shared, str(nm)) == expected_symbols
    env = environment()
    env["PKG_CONFIG_LIBDIR"] = str(prefix / "lib/pkgconfig")
    env.pop("PKG_CONFIG_PATH", None)
    assert run(["pkg-config", "--modversion", "serde-ucl"], env=env).strip() == tomllib.loads(
        (source / "capi/Cargo.toml").read_text())["package"]["version"]
    with tempfile.TemporaryDirectory(prefix="c15-packaged-") as temp:
        work = Path(temp)
        signatures = work / "signatures.c"
        signatures.write_text(check.signature_source(functions))
        cases = json.loads((spec / "cases.json").read_text())
        for linkage in ("shared", "static"):
            args = ["pkg-config", "--cflags", "--libs"]
            if linkage == "static":
                args.append("--static")
            flags = shlex.split(run([*args, "serde-ucl"], env=env))
            if linkage == "static":
                flags = [str(prefix / "lib/libucl.a") if x == "-lucl" else x for x in flags]
            else:
                flags.append(f"-Wl,-rpath,{prefix / 'lib'}")
            for compiler, language, standard in (("cc", "c", "c11"), ("c++", "c++", "c++11")):
                binary = work / f"signatures-{linkage}-{language}"
                cflags = shlex.split(run(["pkg-config", "--cflags", "serde-ucl"], env=env))
                obj = binary.with_suffix(".o")
                run([compiler, "-x", language, f"-std={standard}", "-Wall", "-Wextra", "-Werror",
                     "-c", signatures, *cflags, "-o", obj], env=env)
                run([compiler, obj, *flags, "-o", binary], env=env)
                run([binary], cwd=work, env=env)
            binary = work / f"probe-{linkage}"
            run(["cc", "-std=c11", "-D_POSIX_C_SOURCE=200809L", "-Wall", "-Wextra", "-Werror",
                 source / cases["source"], *flags, "-o", binary], env=env)
            for case in cases["cases"]:
                case_dir = work / f"{linkage}-{case}"
                case_dir.mkdir()
                result = run([binary, case], cwd=case_dir, env=env).encode()
                assert result == (source / cases["golden"] / "darwin-arm64" / f"{case}.txt").read_bytes(), case
            extra = work / f"extra-{linkage}"
            run(["cc", "-std=c11", "-D_POSIX_C_SOURCE=200809L", source / "capi/tests/extra.c",
                 *flags, "-o", extra], env=env)
            run([extra], cwd=work, env=env)
    print(f"{prefix}: exact header, ABI 1, macOS 11 arm64, 43 C/C++ signatures, ten cases and extra tests passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/c15-dist")
    parser.add_argument("--require-clean", action="store_true", help="reject dirty release inputs")
    parser.add_argument("--verify", action="store_true", help="extract, relocate, link and clean-build archives")
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("Archive verification requires Python assertions enabled")
    require_tools()
    if args.require_clean and run(["git", "status", "--porcelain"]).strip():
        raise RuntimeError("Release distribution requires a clean tested commit")
    root_version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if root_version != tomllib.loads((ROOT / "capi/Cargo.toml").read_text())["package"]["version"]:
        raise RuntimeError("C distribution and Rust release versions differ")
    if (ROOT / "capi/include/ucl.h").read_bytes() != subprocess.check_output(
            ["git", "show", f"spec-v22:{SPEC}/include/ucl.h"], cwd=ROOT):
        raise RuntimeError("Shipping header differs from spec-v22")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT / "capi/Cargo.toml").read_text())["package"]["version"]
    expected_outputs = {f"serde-ucl-c-{version}-source.tar.gz",
                        f"serde-ucl-c-{version}-macos-arm64.tar.gz", "SHA256SUMS"}
    unexpected = {p.name for p in output.iterdir()} - expected_outputs
    if unexpected:
        raise RuntimeError(f"Use a dedicated output directory; unexpected entries: {sorted(unexpected)}")
    commit = run(["git", "rev-parse", "HEAD"]).strip()
    if args.require_clean and os.environ.get("GITHUB_SHA", commit) != commit:
        raise RuntimeError("Checkout commit differs from the release workflow commit")
    with tempfile.TemporaryDirectory(prefix="c15-distribution-") as temp:
        work = Path(temp)
        source = work / f"serde-ucl-c-{version}-source"
        source.mkdir()
        source_tree(source)
        third_party_licenses(source)
        provenance = {"commit": commit, "rust_package_version": version, "abi_version": ABI,
                      "spec": "spec-v22", "cargo_c": CARGO_C, "target": "aarch64-apple-darwin",
                      "minimum_macos": "11.0", "source_manifest_adjustment": "omit benchmark targets",
                      "dirty": bool(run(["git", "status", "--porcelain"]).strip()),
                      "rustc": run(["rustc", "--version"]).strip()}
        (source / "BUILD-INFO.json").write_text(json.dumps(provenance, indent=2) + "\n")
        checksums(source)
        source_archive = output / f"{source.name}.tar.gz"
        archive(source, source_archive)
        sdk = work / f"serde-ucl-c-{version}-macos-arm64"
        install(source, sdk)
        relocate(sdk)
        for name in ("LICENSE-MIT", "LICENSE-APACHE", "BUILD-INFO.json"):
            shutil.copy2(source / name, sdk / name)
        shutil.copy2(source / "capi/README.md", sdk / "README.md")
        shutil.copytree(source / "licenses", sdk / "licenses")
        checksums(sdk)
        sdk_archive = output / f"{sdk.name}.tar.gz"
        archive(sdk, sdk_archive)
        # Prove links use extracted/relocated SDK rather than its build prefix.
        shutil.rmtree(sdk)
        if args.verify:
            extracted = work / "relocated path"
            extracted.mkdir()
            for item in (source_archive, sdk_archive):
                with tarfile.open(item) as tar:
                    tar.extractall(extracted, filter="data")
            extracted_source = extracted / source.name
            extracted_sdk = extracted / sdk.name
            verify_checksums(extracted_source)
            verify_checksums(extracted_sdk)
            assert json.loads((extracted_source / "BUILD-INFO.json").read_text()) == provenance
            assert json.loads((extracted_sdk / "BUILD-INFO.json").read_text()) == provenance
            sdk_files = {str(p.relative_to(extracted_sdk)) for p in extracted_sdk.rglob("*")
                         if p.is_file() and not str(p.relative_to(extracted_sdk)).startswith("licenses/")}
            assert sdk_files == {"BUILD-INFO.json", "README.md", "SHA256SUMS", "LICENSE-MIT",
                                 "LICENSE-APACHE", "include/ucl.h", "lib/libucl.a",
                                 "lib/libucl.dylib", "lib/libucl.1.dylib", "lib/libucl.1.0.0.dylib",
                                 "lib/pkgconfig/serde-ucl.pc"}, sdk_files
            assert (extracted_sdk / "licenses/rust/COPYRIGHT-library.html").is_file()
            assert (extracted_sdk / "lib/libucl.dylib").readlink() == Path("libucl.1.0.0.dylib")
            assert (extracted_sdk / "lib/libucl.1.dylib").readlink() == Path("libucl.1.0.0.dylib")
            verify_library(extracted_sdk, extracted_source, portable=True)
            install(extracted_source, work / "clean-install")
            verify_library(work / "clean-install", extracted_source, portable=False)
    (output / "SHA256SUMS").write_text("".join(
        f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in (source_archive, sdk_archive)))
    print(f"Archives and checksums: {output}")


if __name__ == "__main__":
    main()
