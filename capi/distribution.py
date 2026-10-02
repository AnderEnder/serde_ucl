#!/usr/bin/env python3
"""Build and verify native opt-in C source/SDK distributions (no publishing)."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

from distribution_platform import (TARGETS, build_environment, host_metadata, inspect_consumer,
                                   inspect_library, native_target, version_tuple)

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


def environment(target=None):
    return build_environment(target or native_target())


def require_tools():
    if CARGO_C not in run(["cargo", "cinstall", "--version"]):
        raise RuntimeError(f"Install cargo-c {CARGO_C} with --locked")


def install(source, prefix, target):
    env = environment(target)
    env["CARGO_TARGET_DIR"] = str(source / "capi/target/distribution")
    run(["cargo", "cinstall", "--locked", "--release", "--manifest-path",
         source / "capi/Cargo.toml", "--target", target.rust,
         "--prefix", prefix, "--libdir", prefix / "lib"], cwd=source, env=env)


def relocate(prefix, target):
    if target.system == "Darwin":
        run(["install_name_tool", "-id", "@rpath/" + target.versioned,
             prefix / "lib" / target.versioned])
    pc = prefix / "lib/pkgconfig/serde-ucl.pc"
    lines = pc.read_text().splitlines()
    replacements = {"prefix": "${pcfiledir}/../..", "exec_prefix": "${prefix}",
                    "libdir": "${prefix}/lib", "includedir": "${prefix}/include"}
    pc.write_text("\n".join(next((f"{key}={value}" for key, value in replacements.items()
                                    if line.startswith(key + "=")), line)
                            for line in lines) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def golden_evidence(source, target, golden_dir=None, candidate=None):
    spec = source / SPEC
    inventory = json.loads((spec / "api.json").read_text())
    cases = json.loads((spec / "cases.json").read_text())
    golden = golden_dir or source / cases["golden"] / target.golden
    if target.system == "Linux" and golden_dir is None:
        raise RuntimeError("Linux verification requires --golden-dir with actual native reference evidence")
    hashes = {case: digest(golden / f"{case}.txt") for case in cases["cases"]}
    if golden_dir is None:
        return golden, {"origin": "released spec-v22 snapshots", "platform": target.golden,
                        "cases": hashes}
    evidence = json.loads((golden / "reference-evidence.json").read_text())
    expected = {"reference_commit": inventory["reference_commit"],
                "reference_header_sha256": inventory["reference_header_sha256"],
                "platform": target.golden, "machine": target.machine, "system": target.system,
                "contract_sha256": digest(spec / "api.json"),
                "probe_sha256": digest(source / cases["source"]), "cases": hashes,
                "provided_functions": 43, "header_library_combinations_per_candidate": 4}
    for key, value in expected.items():
        if evidence.get(key) != value:
            raise RuntimeError(f"Reference evidence mismatch for {key}: {evidence.get(key)!r}")
    if candidate is not None:
        libraries = {str((candidate / "lib" / name).resolve()): digest(candidate / "lib" / name)
                     for name in ("libucl.a", target.shared)}
        if evidence.get("candidate_libraries") != libraries:
            raise RuntimeError("Reference evidence does not match staged candidate library hashes")
    return golden, evidence


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
             "capi/Makefile", "capi/distribution.py", "capi/distribution_platform.py",
             "capi/collect_distribution.py", "capi/IMPLEMENTATION.md",
             "capi/tests/check.py", "capi/tests/extra.c", "capi/tests/distribution_test.py",
             "capi/include/ucl.h",
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


def verify_library(prefix, source, target, golden, *, portable):
    spec = source / SPEC
    assert (prefix / "include/ucl.h").read_bytes() == (spec / "include/ucl.h").read_bytes()
    assert (prefix / "lib/libucl.a").is_file()
    assert (prefix / "lib" / target.shared).is_symlink()
    metadata = inspect_library(prefix, target, run, portable=portable)
    consumer_floors = []
    spec_module = importlib.util.spec_from_file_location("capi_check", source / "capi/tests/check.py")
    check = importlib.util.module_from_spec(spec_module)
    spec_module.loader.exec_module(check)
    functions = [x for x in json.loads((spec / "api.json").read_text())["functions"] if x["provided"]]
    expected_symbols = {x["name"] for x in functions}
    assert len(expected_symbols) == 43
    sysroot = Path(run(["rustc", "--print=sysroot"]).strip())
    nm = sysroot / "lib/rustlib" / target.rust / "bin/llvm-nm"
    for library, shared in [(prefix / "lib/libucl.a", False), (prefix / "lib" / target.versioned, True)]:
        assert check.symbols(library, shared, str(nm)) == expected_symbols
    env = environment(target)
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
                if linkage == "static" and target.system == "Linux":
                    consumer_floors.append(inspect_consumer(binary, target, run))
            binary = work / f"probe-{linkage}"
            run(["cc", "-std=c11", "-D_POSIX_C_SOURCE=200809L", "-Wall", "-Wextra", "-Werror",
                 source / cases["source"], *flags, "-o", binary], env=env)
            if linkage == "static" and target.system == "Linux":
                consumer_floors.append(inspect_consumer(binary, target, run))
            for case in cases["cases"]:
                case_dir = work / f"{linkage}-{case}"
                case_dir.mkdir()
                result = run([binary, case], cwd=case_dir, env=env).encode()
                assert result == (golden / f"{case}.txt").read_bytes(), case
            extra = work / f"extra-{linkage}"
            run(["cc", "-std=c11", "-D_POSIX_C_SOURCE=200809L", source / "capi/tests/extra.c",
                 *flags, "-o", extra], env=env)
            run([extra], cwd=work, env=env)
            if linkage == "static" and target.system == "Linux":
                consumer_floors.append(inspect_consumer(extra, target, run))
    if consumer_floors:
        metadata["static_consumer_glibc_symbol_floor"] = max(consumer_floors, key=version_tuple)
        metadata["glibc_symbol_floor"] = max(metadata["shared_glibc_symbol_floor"],
                                             metadata["static_consumer_glibc_symbol_floor"],
                                             key=version_tuple)
    print(f"{prefix}: {target.name}, exact header, ABI 1, architecture/deployment metadata, "
          "43 C/C++ signatures, ten target snapshots and extra tests passed")
    return metadata


def verify_contents(sdk, target):
    files = {str(p.relative_to(sdk)) for p in sdk.rglob("*") if p.is_file()
             and not str(p.relative_to(sdk)).startswith(("licenses/", "conformance/"))}
    assert files == {"BUILD-INFO.json", "README.md", "SHA256SUMS", "LICENSE-MIT",
                     "LICENSE-APACHE", "include/ucl.h", "lib/libucl.a",
                     "lib/" + target.shared, "lib/" + target.versioned, "lib/" + target.full,
                     "lib/pkgconfig/serde-ucl.pc"}, files
    assert (sdk / "licenses/rust/COPYRIGHT-library.html").is_file()
    for name in (target.shared, target.versioned):
        assert (sdk / "lib" / name).readlink() == Path(target.full)


def clean_inputs(required):
    if required and run(["git", "status", "--porcelain"]).strip():
        raise RuntimeError("Release distribution requires a clean tested commit")
    version = tomllib.loads((ROOT / "capi/Cargo.toml").read_text())["package"]["version"]
    if version != tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]:
        raise RuntimeError("C distribution and Rust release versions differ")
    if (ROOT / "capi/include/ucl.h").read_bytes() != subprocess.check_output(
            ["git", "show", f"spec-v22:{SPEC}/include/ucl.h"], cwd=ROOT):
        raise RuntimeError("Shipping header differs from spec-v22")
    commit = run(["git", "rev-parse", "HEAD"]).strip()
    if required and os.environ.get("GITHUB_SHA", commit) != commit:
        raise RuntimeError("Checkout commit differs from the release workflow commit")
    return version, commit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, help="require this native host (no cross builds)")
    parser.add_argument("--assert-native", action="store_true", help="only assert OS/architecture")
    parser.add_argument("--kind", choices=("source", "sdk", "all"), default="all")
    parser.add_argument("--output", type=Path, default=ROOT / "target/c15-dist")
    parser.add_argument("--candidate-prefix", type=Path, help="staged candidate checked by reference CLI")
    parser.add_argument("--build-only", action="store_true", help="install candidate for black-box reference check")
    parser.add_argument("--golden-dir", type=Path, help="actual native snapshots with reference-evidence.json")
    parser.add_argument("--require-clean", action="store_true", help="reject dirty release inputs")
    parser.add_argument("--verify", action="store_true", help="extract, relocate, link and clean-build archives")
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("Archive verification requires Python assertions enabled")
    target = native_target(args.target)
    if args.assert_native:
        print(f"Native runner verified: {target.name} / {target.rust}")
        return
    require_tools()
    version, commit = clean_inputs(args.require_clean)
    base = {"commit": commit, "rust_package_version": version, "abi_version": ABI,
            "spec": "spec-v22", "cargo_c": CARGO_C, "source_manifest_adjustment": "omit benchmark targets",
            "dirty": bool(run(["git", "status", "--porcelain"]).strip()),
            "rustc": run(["rustc", "--version"]).strip()}
    candidate = args.candidate_prefix.resolve() if args.candidate_prefix else None
    if args.build_only:
        if candidate is None:
            parser.error("--build-only requires --candidate-prefix")
        install(ROOT, candidate, target)
        (candidate / "CANDIDATE-INFO.json").write_text(json.dumps(dict(base, target=target.rust), indent=2) + "\n")
        print(f"Candidate staged for reference check: {candidate}")
        return
    golden, evidence = golden_evidence(ROOT, target, args.golden_dir, candidate)
    if candidate is not None:
        info = json.loads((candidate / "CANDIDATE-INFO.json").read_text())
        if any(info.get(key) != value for key, value in dict(base, target=target.rust).items()):
            raise RuntimeError("Candidate was not built from this commit/toolchain/target")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    names = []
    if args.kind in ("source", "all"):
        names.append(f"serde-ucl-c-{version}-source.tar.gz")
    if args.kind in ("sdk", "all"):
        names.append(f"serde-ucl-c-{version}-{target.name}.tar.gz")
    checksum_name = "SHA256SUMS" if args.kind == "all" else "SHA256SUMS-" + (
        "source" if args.kind == "source" else target.name)
    unexpected = {p.name for p in output.iterdir()} - set(names) - {checksum_name}
    if unexpected:
        raise RuntimeError(f"Use a dedicated output directory; unexpected entries: {sorted(unexpected)}")
    with tempfile.TemporaryDirectory(prefix="c15-distribution-") as temp:
        work = Path(temp)
        source = work / f"serde-ucl-c-{version}-source"
        source.mkdir()
        source_tree(source)
        third_party_licenses(source)
        source_info = dict(base, kind="source", target=None, verified=args.verify)
        (source / "BUILD-INFO.json").write_text(json.dumps(source_info, indent=2) + "\n")
        checksums(source)
        if args.kind in ("source", "all"):
            archive(source, output / f"{source.name}.tar.gz")
        sdk = work / f"serde-ucl-c-{version}-{target.name}"
        if args.kind in ("sdk", "all"):
            if candidate is None:
                install(source, sdk, target)
            else:
                shutil.copytree(candidate / "lib", sdk / "lib", symlinks=True)
                shutil.copytree(candidate / "include", sdk / "include")
            relocate(sdk, target)
            metadata = verify_library(sdk, source, target, golden, portable=True)
            sdk_info = dict(base, kind="sdk", target=target.rust, verified=args.verify,
                            native_host=host_metadata(target, run), deployment=metadata,
                            conformance=evidence)
            if target.system == "Darwin":
                sdk_info["minimum_macos"] = "11.0"
            (sdk / "BUILD-INFO.json").write_text(json.dumps(sdk_info, indent=2) + "\n")
            for name in ("LICENSE-MIT", "LICENSE-APACHE"):
                shutil.copy2(source / name, sdk / name)
            shutil.copy2(source / "capi/README.md", sdk / "README.md")
            shutil.copytree(source / "licenses", sdk / "licenses")
            conformance = sdk / "conformance"
            conformance.mkdir()
            cases = json.loads((source / SPEC / "cases.json").read_text())["cases"]
            for case in cases:
                shutil.copy2(golden / f"{case}.txt", conformance / f"{case}.txt")
            (conformance / "reference-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
            checksums(sdk)
            archive(sdk, output / f"{sdk.name}.tar.gz")
            shutil.rmtree(sdk)  # Prove extracted libraries work after removal of build prefix.
        if args.verify:
            extracted = work / "relocated path"
            extracted.mkdir()
            for name in names:
                with tarfile.open(output / name) as tar:
                    tar.extractall(extracted, filter="data")
            if args.kind in ("source", "all"):
                extracted_source = extracted / source.name
                verify_checksums(extracted_source)
                assert json.loads((extracted_source / "BUILD-INFO.json").read_text()) == source_info
            else:
                # Serialize/extract a private source input to test a clean path-dependency build;
                # SDK jobs do not emit a duplicate release source archive.
                private = work / "source-input.tar.gz"
                archive(source, private)
                with tarfile.open(private) as tar:
                    tar.extractall(extracted, filter="data")
                extracted_source = extracted / source.name
                verify_checksums(extracted_source)
            if args.kind in ("sdk", "all"):
                extracted_sdk = extracted / sdk.name
                verify_checksums(extracted_sdk)
                assert json.loads((extracted_sdk / "BUILD-INFO.json").read_text()) == sdk_info
                verify_contents(extracted_sdk, target)
                extracted_golden, _ = golden_evidence(extracted_source, target,
                    extracted_sdk / "conformance" if args.golden_dir else None)
                measured = verify_library(extracted_sdk, extracted_source, target, extracted_golden, portable=True)
                assert measured == metadata, "Deployment metadata changed after extraction/relocation"
            install(extracted_source, work / "clean-install", target)
            verify_library(work / "clean-install", extracted_source, target, golden, portable=False)
    (output / checksum_name).write_text("".join(
        f"{digest(output / name)}  {name}\n" for name in names))
    print(f"Archives and checksums: {output}")


if __name__ == "__main__":
    main()
