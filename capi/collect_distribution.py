#!/usr/bin/env python3
"""Validate one source + three native SDK artifacts and consolidate release checksums."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tomllib

from distribution_platform import TARGETS, version_tuple

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_archive(tar, path):
    member = tar.getmember(path)
    if not member.isfile():
        raise RuntimeError(f"Expected regular archive file: {path}")
    return tar.extractfile(member).read()


def collect(incoming, output, version, commit):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise RuntimeError("Expected concrete release version and full commit SHA")
    kinds = ["source", *TARGETS]
    archive_names = {kind: f"serde-ucl-capi-{version}-{kind}.tar.gz" for kind in kinds}
    expected = set(archive_names.values()) | {"SHA256SUMS-" + kind for kind in kinds}
    actual = {p.name for p in incoming.iterdir()}
    if actual != expected:
        raise RuntimeError(f"Incomplete/unexpected release inputs: missing {sorted(expected - actual)}, "
                           f"extra {sorted(actual - expected)}")
    inventory = json.loads((ROOT / "docs/spec/c-api/stage-a/api.json").read_text())
    cases = json.loads((ROOT / "docs/spec/c-api/stage-a/cases.json").read_text())["cases"]
    for kind, name in archive_names.items():
        checksum = incoming / ("SHA256SUMS-" + kind)
        expected_line = f"{digest(incoming / name)}  {name}\n"
        if checksum.read_text() != expected_line:
            raise RuntimeError(f"Invalid checksum manifest for {kind}")
        with tarfile.open(incoming / name) as tar:
            directory = name.removesuffix(".tar.gz")
            info = json.loads(read_archive(tar, directory + "/BUILD-INFO.json"))
            required = {"commit": commit, "rust_package_version": version, "abi_version": "1.0.0",
                        "spec": "spec-v22", "cargo_c": "0.10.25+cargo-0.99.0", "dirty": False, "verified": True,
                        "kind": "source" if kind == "source" else "sdk",
                        "target": None if kind == "source" else TARGETS[kind].rust}
            for key, value in required.items():
                if info.get(key) != value:
                    raise RuntimeError(f"{kind}: provenance mismatch for {key}: {info.get(key)!r}")
            if kind == "source":
                continue
            target = TARGETS[kind]
            deployment = info["deployment"]
            if target.system == "Linux":
                if deployment.get("format") != "ELF64" or deployment.get("machine") != target.elf_machine:
                    raise RuntimeError(f"{kind}: ELF architecture metadata mismatch")
                if deployment.get("soname") != target.versioned:
                    raise RuntimeError(f"{kind}: SONAME mismatch")
                for key in ("glibc_symbol_floor", "shared_glibc_symbol_floor", "static_consumer_glibc_symbol_floor"):
                    if not re.fullmatch(r"\d+(?:\.\d+)+", deployment.get(key, "")):
                        raise RuntimeError(f"{kind}: missing actual GLIBC requirements")
                floor = max(deployment["shared_glibc_symbol_floor"],
                            deployment["static_consumer_glibc_symbol_floor"], key=version_tuple)
                if deployment["glibc_symbol_floor"] != floor:
                    raise RuntimeError(f"{kind}: incorrect aggregate GLIBC floor")
                if version_tuple(info["native_host"]["glibc"]) < version_tuple(floor):
                    raise RuntimeError(f"{kind}: symbol floor exceeds tested glibc")
                if "minimum_macos" in info or "minimum_macos" in deployment:
                    raise RuntimeError(f"{kind}: Darwin deployment metadata on Linux SDK")
            elif (deployment.get("format") != "Mach-O" or deployment.get("machine") != "arm64"
                  or deployment.get("minimum_macos") != "11.0"
                  or deployment.get("install_name") != "@rpath/libucl.1.dylib"):
                raise RuntimeError("Darwin SDK deployment/identity mismatch")
            if (info["native_host"]["system"], info["native_host"]["machine"]) != (target.system, target.machine):
                raise RuntimeError(f"{kind}: native host mismatch")
            evidence = info["conformance"]
            expected_evidence = {"reference_commit": inventory["reference_commit"],
                                 "reference_header_sha256": inventory["reference_header_sha256"],
                                 "platform": target.golden, "machine": target.machine, "system": target.system,
                                 "contract_sha256": digest(ROOT / "docs/spec/c-api/stage-a/api.json"),
                                 "probe_sha256": digest(ROOT / "tests/conformance/capi/stage-a/probe.c"),
                                 "provided_functions": 43,
                                 "header_library_combinations_per_candidate": 4}
            for key, value in expected_evidence.items():
                if evidence.get(key) != value:
                    raise RuntimeError(f"{kind}: reference evidence mismatch for {key}")
            if set(evidence["cases"]) != set(cases):
                raise RuntimeError(f"{kind}: incomplete native case evidence")
            for case, expected_digest in evidence["cases"].items():
                data = read_archive(tar, directory + f"/conformance/{case}.txt")
                if hashlib.sha256(data).hexdigest() != expected_digest:
                    raise RuntimeError(f"{kind}: native snapshot digest mismatch for {case}")
            if json.loads(read_archive(tar, directory + "/conformance/reference-evidence.json")) != evidence:
                raise RuntimeError(f"{kind}: bundled evidence differs from provenance")
    output.mkdir(parents=True, exist_ok=True)
    permitted = set(archive_names.values()) | {"SHA256SUMS"}
    if {p.name for p in output.iterdir()} - permitted:
        raise RuntimeError("Use an isolated release output directory")
    for name in archive_names.values():
        shutil.copy2(incoming / name, output / name)
    (output / "SHA256SUMS").write_text("".join(
        f"{digest(output / name)}  {name}\n" for name in sorted(archive_names.values())))
    print(f"Collected same-commit source and three native SDKs: {output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--version", default=tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"])
    args = parser.parse_args()
    collect(args.input.resolve(), args.output.resolve(), args.version, args.commit)


if __name__ == "__main__":
    main()
