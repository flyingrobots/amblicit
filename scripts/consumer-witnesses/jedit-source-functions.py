"""Public Jim source-function build compatibility with an explicitly selected old provider.

Run only in an admitted, resource-guarded reusable Docker worker. This program
creates no worker/cache, builds no compiler, and does not modify provider inputs.
"""

import argparse
import hashlib
import json
import os
import resource
import signal
from pathlib import Path
import shutil
import subprocess

MAX_INPUT_BYTES = 16 * 1024 * 1024
MAX_LOG_BYTES = 1024 * 1024


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def files(root):
    return {str(path.relative_to(root)): digest(path)
            for path in sorted(root.rglob("*")) if path.is_file()}


def verify_source(root, manifest, revision):
    if manifest["head"] != revision:
        raise RuntimeError("Compiler source revision differs from explicit selection")
    for name, expected in manifest["files"].items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise RuntimeError("Unsafe source manifest path")
        if digest(root / relative) != expected:
            raise RuntimeError(f"Compiler source changed: {name}")


def child_limits():
    # This also limits each emitted artifact, not just the two log files.
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_LOG_BYTES, MAX_LOG_BYTES))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def stop_group(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass
    # Kill remaining descendants even if their parent already exited.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def build(binary, root):
    request = {"schema": "edict.compiler.settings/v1", "type": "compilerSettings",
               "operation": "build", "application": "edict.application.json"}
    streams = [root / "stdout.jsonl", root / "stderr.jsonl"]
    with streams[0].open("wb") as stdout, streams[1].open("wb") as stderr:
        child = subprocess.Popen([str(binary)], cwd=root, stdin=subprocess.PIPE,
                                 stdout=stdout, stderr=stderr, start_new_session=True,
                                 preexec_fn=child_limits)
        try:
            child.communicate((json.dumps(request) + "\n").encode(), timeout=120)
        finally:
            # Stop surviving descendants before reading or hashing their output.
            stop_group(child)
        result = child
    events = []
    for path in streams:
        if path.stat().st_size > MAX_LOG_BYTES:
            raise RuntimeError("Compiler logs exceed witness ceiling")
        events.extend(json.loads(line) for line in path.read_text().splitlines())
    status = [event for event in events if event.get("type") == "status"]
    if len(status) != 1 or status[0].get("exitCode") != result.returncode:
        raise RuntimeError("Compiler status stream disagrees with process exit")
    output = root / "application-output"
    return {"exitCode": result.returncode,
            "diagnostics": [event for event in events if event.get("type") == "diagnostic"],
            "artifacts": files(output),
            "sourceSha256": digest(root / "src/RangeAssembly.edict"),
            "stdoutSha256": digest(streams[0]), "stderrSha256": digest(streams[1])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("compiler", "compiler-source", "compiler-manifest", "application-source",
                 "provider-package", "data-root", "work-root"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--compiler-revision", required=True)
    parser.add_argument("--provider-manifest-sha256", required=True)
    args = parser.parse_args()
    if not Path("/.dockerenv").is_file():
        raise RuntimeError("This witness requires the shared guarded Docker worker")
    if not args.work_root.resolve().is_relative_to(args.data_root.resolve()):
        raise RuntimeError("Work must remain inside the caller's guarded data root")
    manifest = json.loads(args.compiler_manifest.read_text())
    verify_source(args.compiler_source, manifest, args.compiler_revision)
    provider_manifest = args.provider_package / "provider-manifest.echo.json"
    if digest(provider_manifest) != args.provider_manifest_sha256:
        raise RuntimeError("Provider manifest differs from explicit selection")
    vendor = args.application_source / "vendor"
    total = sum(path.stat().st_size for root in [vendor, args.provider_package]
                for path in root.rglob("*") if path.is_file())
    if total > MAX_INPUT_BYTES:
        raise RuntimeError("Selected provider and lawpack inputs exceed the bounded copy budget")
    original = {"vendor": files(vendor), "provider": files(args.provider_package),
                "application": digest(args.application_source / "edict.application.json"),
                "compiler": digest(args.compiler)}
    args.work_root.mkdir()
    fixture = args.compiler_source / "fixtures/lang/functions"
    evidence = {"compilerRevision": args.compiler_revision,
                "compilerManifestSha256": digest(args.compiler_manifest),
                "compilerBinarySha256": original["compiler"],
                "providerManifestSha256": args.provider_manifest_sha256,
                "results": {}}
    for name, source in [("function-free", "range-assembly-baseline.edict"),
                         ("source-function", "range-assembly.edict")]:
        root = args.work_root / name
        (root / "src").mkdir(parents=True)
        shutil.copytree(vendor, root / "vendor")
        shutil.copytree(args.provider_package, root / "provider")
        shutil.copyfile(fixture / source, root / "src/RangeAssembly.edict")
        application = json.loads((args.application_source / "edict.application.json").read_text())
        application["sources"] = ["src/RangeAssembly.edict"]
        application["target"]["providerPackage"] = "provider"
        application["outputDirectory"] = "application-output"
        (root / "edict.application.json").write_text(json.dumps(application, indent=2) + "\n")
        evidence["results"][name] = build(args.compiler.resolve(), root)
    (args.work_root / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    for name, result in evidence["results"].items():
        print(json.dumps({"case": name, **result}, sort_keys=True), flush=True)
    current = {"vendor": files(vendor), "provider": files(args.provider_package),
               "application": digest(args.application_source / "edict.application.json"),
               "compiler": digest(args.compiler)}
    if current != original:
        raise RuntimeError("An authoritative input changed during the witness")
    verify_source(args.compiler_source, manifest, args.compiler_revision)
    control = evidence["results"]["function-free"]
    if (control["exitCode"] != 0 or control["diagnostics"] or
            sorted(control["artifacts"]) != ["executable-operation-package.cbor", "verification-report.cbor"]):
        raise RuntimeError("Function-free Jim control did not produce a verified package")
    changed = evidence["results"]["source-function"]
    diagnostics = changed["diagnostics"]
    if (changed["exitCode"] != 2 or changed["artifacts"] or len(diagnostics) != 1
            or diagnostics[0].get("kind") != "InvalidProviderInvocation"
            or "ArtifactSchemaMismatch" not in diagnostics[0].get("message", "")):
        raise RuntimeError("Source function did not explicitly refuse the old provider schema")
    print("JIM_SOURCE_FUNCTION_OLD_PROVIDER_REFUSAL_CONFIRMED", flush=True)


if __name__ == "__main__":
    main()
