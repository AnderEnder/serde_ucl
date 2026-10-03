#!/usr/bin/env python3
"""Packaging/ELF metadata fixtures; these do not execute Linux or establish C conformance."""
import hashlib
import importlib.util
import io
import json
import shlex
import subprocess
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

CAPI = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(CAPI))
import collect_distribution as collector
import distribution
from distribution_platform import TARGETS, build_environment, elf_metadata, glibc_floor, native_target


HEADER = """ELF Header:
  Class:                             ELF64
  Data:                              2's complement, little endian
  Type:                              DYN (Shared object file)
  Machine:                           Advanced Micro Devices X86-64
"""
DYNAMIC = """ 0x000000000000000e (SONAME)             Library soname: [libucl.so.1]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [libgcc_s.so.1]
"""
VERSIONS = "Name: GLIBC_2.9\nName: GLIBC_2.2.5\nName: GLIBC_2.34\n"


class InstallPathTests(unittest.TestCase):
    def test_make_install_explicit_libdir_and_staging_paths(self):
        with tempfile.TemporaryDirectory(prefix="c-install-argv-") as temp:
            work = Path(temp)
            cargo = work / "fake cargo"
            recorded = work / "arguments.json"
            cargo.write_text("#!/usr/bin/env python3\n"
                             "import json, sys\nfrom pathlib import Path\n"
                             f"Path({str(recorded)!r}).write_text(json.dumps(sys.argv[1:]))\n")
            cargo.chmod(0o755)
            prefix = work / "install prefix"
            staged = work / "staged root"
            for override, destdir in ((None, None), (None, staged), (prefix / "lib64", staged)):
                command = ["make", "-C", str(CAPI), "install", f"PREFIX={prefix}",
                           f"CARGO={shlex.quote(str(cargo))}"]
                if override is not None:
                    command.append(f"LIBDIR={override}")
                if destdir is not None:
                    command.append(f"DESTDIR={destdir}")
                subprocess.run(command, check=True, capture_output=True)
                arguments = json.loads(recorded.read_text())
                with self.subTest(libdir=override, destdir=destdir):
                    self.assertEqual(arguments[0], "cinstall")
                    self.assertEqual(arguments[arguments.index("--prefix") + 1], str(prefix))
                    self.assertIn("--libdir", arguments)
                    self.assertEqual(arguments[arguments.index("--libdir") + 1],
                                     str(override or prefix / "lib"))
                    if destdir is not None:
                        self.assertEqual(arguments[arguments.index("--destdir") + 1], str(destdir))
                    else:
                        self.assertNotIn("--destdir", arguments)


class NativeLinkTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location("capi_check", CAPI / "tests/check.py")
        cls.check = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.check)

    def test_plain_and_colored_linux_darwin_dependencies(self):
        for flags in ("-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc",
                      "-liconv -lSystem -lc -lm"):
            for output in (f"note: native-static-libs: {flags}\n",
                           f"\x1b[1m\x1b[32mnote\x1b[0m: \x1b[1mnative-static-libs: {flags}\x1b[0m\n"
                           "\x1b[1m\x1b[32m    Finished\x1b[0m release build\n"):
                with self.subTest(output=output):
                    self.assertEqual(self.check.native_static_libraries(output), flags.split())

    def test_embedded_styling_and_quoted_link_arguments(self):
        output = ('note: native-static-libs: \x1b[38;5;2m-lc\x1b[0m '
                  '\x1b[1m-lm\x1b[0m "library path/libextra.a"\r\n')
        self.assertEqual(self.check.native_static_libraries(output),
                         ["-lc", "-lm", "library path/libextra.a"])

    def test_missing_or_empty_diagnostic_rejects(self):
        for output in ("Finished release build\n", "note: native-static-libs: \x1b[0m\n",
                       "note: native-static-libs:\n    Finished release build\n"):
            with self.subTest(output=output):
                with self.assertRaises(RuntimeError):
                    self.check.native_static_libraries(output)


class PlatformTests(unittest.TestCase):
    def test_native_matrix_and_cross_host_rejection(self):
        for target in TARGETS.values():
            self.assertEqual(native_target(target.name, system=target.system, machine=target.machine), target)
        with self.assertRaises(RuntimeError):
            native_target("linux-arm64", system="Linux", machine="x86_64")
        with self.assertRaises(RuntimeError):
            native_target("linux-amd64", system="Darwin", machine="arm64")

    def test_deployment_environment_is_platform_specific(self):
        self.assertEqual(build_environment(TARGETS["macos-arm64"])["MACOSX_DEPLOYMENT_TARGET"], "11.0")
        self.assertNotIn("MACOSX_DEPLOYMENT_TARGET", build_environment(TARGETS["linux-amd64"]))

    def test_elf_machines_and_glibc_numeric_floor(self):
        self.assertEqual(glibc_floor(VERSIONS), "2.34")
        amd64 = elf_metadata(HEADER, DYNAMIC, VERSIONS, TARGETS["linux-amd64"])
        arm64 = elf_metadata(HEADER.replace("Advanced Micro Devices X86-64", "AArch64"),
                             DYNAMIC, VERSIONS, TARGETS["linux-arm64"])
        self.assertEqual(amd64["shared_glibc_symbol_floor"], "2.34")
        self.assertEqual(arm64["machine"], "AArch64")

    def test_malformed_architecture_soname_paths_and_glibc_reject(self):
        for header, dynamic, versions in [
                (HEADER, DYNAMIC.replace("libucl.so.1", "libucl.so.0"), VERSIONS),
                (HEADER.replace("ELF64", "ELF32"), DYNAMIC, VERSIONS),
                (HEADER.replace("X86-64", "AArch64"), DYNAMIC, VERSIONS),
                (HEADER.replace("little endian", "big endian"), DYNAMIC, VERSIONS),
                (HEADER, DYNAMIC + " (RUNPATH) Library runpath: [/build/lib]\n", VERSIONS),
                (HEADER, DYNAMIC.replace("libc.so.6", "/build/libc.so.6"), VERSIONS),
                (HEADER, DYNAMIC, "Name: GLIBC_PRIVATE\n"),
                (HEADER, DYNAMIC, "Name: GLIBC_ABI_DT_RELR\n"),
                (HEADER, DYNAMIC, "No version information found")]:
            with self.subTest(header=header, dynamic=dynamic, versions=versions):
                with self.assertRaises(RuntimeError):
                    elf_metadata(header, dynamic, versions, TARGETS["linux-amd64"])

    def test_linux_never_uses_darwin_snapshots_as_default(self):
        with self.assertRaisesRegex(RuntimeError, "actual native reference evidence"):
            distribution.golden_evidence(distribution.ROOT, TARGETS["linux-amd64"])


class CollectionTests(unittest.TestCase):
    # Synthetic metadata/bytes exercise collection gates only, never C execution.
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="c15-metadata-fixture-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.incoming = self.work / "incoming"
        self.incoming.mkdir()
        self.output = self.work / "output"
        self.commit = "a" * 40
        self.version = "0.6.0"
        self.payloads = {}
        inventory = json.loads((collector.ROOT / "docs/spec/c-api/stage-a/api.json").read_text())
        cases = json.loads((collector.ROOT / "docs/spec/c-api/stage-a/cases.json").read_text())["cases"]
        for kind in ("source", *TARGETS):
            info = {"commit": self.commit, "rust_package_version": self.version,
                    "abi_version": "1.0.0", "spec": "spec-v22", "cargo_c": distribution.CARGO_C,
                    "dirty": False, "verified": True, "kind": "source" if kind == "source" else "sdk",
                    "target": None if kind == "source" else TARGETS[kind].rust}
            files = {}
            if kind != "source":
                target = TARGETS[kind]
                info["native_host"] = {"system": target.system, "machine": target.machine, "glibc": "2.39"}
                info["deployment"] = ({"format": "ELF64", "machine": target.elf_machine,
                                       "soname": target.versioned, "shared_glibc_symbol_floor": "2.34",
                                       "static_consumer_glibc_symbol_floor": "2.38", "glibc_symbol_floor": "2.38"}
                                      if target.system == "Linux" else
                                      {"format": "Mach-O", "machine": "arm64", "minimum_macos": "11.0", "install_name": "@rpath/libucl.1.dylib"})
                evidence = {"reference_commit": inventory["reference_commit"],
                            "reference_header_sha256": inventory["reference_header_sha256"],
                            "platform": target.golden, "machine": target.machine, "system": target.system,
                            "provided_functions": 43,
                            "contract_sha256": collector.digest(collector.ROOT / "docs/spec/c-api/stage-a/api.json"),
                            "probe_sha256": collector.digest(collector.ROOT / "tests/conformance/capi/stage-a/probe.c"),
                            "header_library_combinations_per_candidate": 4, "cases": {}}
                for case in cases:
                    data = b"synthetic packaging fixture; not oracle output\n"
                    files[f"conformance/{case}.txt"] = data
                    evidence["cases"][case] = hashlib.sha256(data).hexdigest()
                info["conformance"] = evidence
                files["conformance/reference-evidence.json"] = json.dumps(evidence).encode()
            self.payloads[kind] = (info, files)
            self.write(kind)

    def write(self, kind):
        info, files = self.payloads[kind]
        name = f"serde-ucl-c-{self.version}-{kind}"
        archive = self.incoming / (name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as tar:
            for path, data in {"BUILD-INFO.json": json.dumps(info).encode(), **files}.items():
                entry = tarfile.TarInfo(name + "/" + path)
                entry.size = len(data)
                tar.addfile(entry, io.BytesIO(data))
        (self.incoming / ("SHA256SUMS-" + kind)).write_text(f"{collector.digest(archive)}  {archive.name}\n")

    def collect(self):
        collector.collect(self.incoming, self.output, self.version, self.commit)

    def test_complete_collection_has_four_archives_one_checksum(self):
        self.collect()
        self.assertEqual(len(list(self.output.iterdir())), 5)
        self.assertEqual(len((self.output / "SHA256SUMS").read_text().splitlines()), 4)

    def test_missing_extra_and_corrupt_inputs_reject(self):
        checksum = self.incoming / "SHA256SUMS-source"
        original = checksum.read_text()
        checksum.unlink()
        with self.assertRaises(RuntimeError):
            self.collect()
        checksum.write_text(original)
        extra = self.incoming / "unexpected.tar.gz"
        extra.write_bytes(b"extra")
        with self.assertRaises(RuntimeError):
            self.collect()
        extra.unlink()
        checksum.write_text(original.replace(original[:64], "0" * 64))
        with self.assertRaises(RuntimeError):
            self.collect()

    def test_native_reference_identity_and_darwin_architecture_reject(self):
        for kind, section, key, invalid in [
                ("linux-arm64", "conformance", "machine", "x86_64"),
                ("linux-arm64", "conformance", "system", "Darwin"),
                ("macos-arm64", "deployment", "format", "ELF64"),
                ("macos-arm64", "deployment", "machine", "x86_64")]:
            info, files = self.payloads[kind]
            original = info[section][key]
            info[section][key] = invalid
            if section == "conformance":
                files["conformance/reference-evidence.json"] = json.dumps(info[section]).encode()
            self.write(kind)
            with self.assertRaises(RuntimeError):
                self.collect()
            info[section][key] = original
            if section == "conformance":
                files["conformance/reference-evidence.json"] = json.dumps(info[section]).encode()
            self.write(kind)

    def test_commit_target_deployment_and_snapshot_mismatches_reject(self):
        info, files = self.payloads["linux-arm64"]
        for key, value in [("commit", "b" * 40), ("target", TARGETS["linux-amd64"].rust),
                           ("dirty", True), ("verified", False)]:
            original = info[key]
            info[key] = value
            self.write("linux-arm64")
            with self.assertRaises(RuntimeError):
                self.collect()
            info[key] = original
        info["deployment"]["glibc_symbol_floor"] = "2.9"
        self.write("linux-arm64")
        with self.assertRaises(RuntimeError):
            self.collect()
        info["deployment"]["glibc_symbol_floor"] = "2.38"
        files["conformance/abi.txt"] = b"tampered structural fixture"
        self.write("linux-arm64")
        with self.assertRaises(RuntimeError):
            self.collect()


if __name__ == "__main__":
    unittest.main()
