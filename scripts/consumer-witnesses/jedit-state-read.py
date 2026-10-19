"""Exercise an authored Jim read/guard through the public compiler in Docker."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def build(binary, root, document, expected_kind):
    request = {
        "schema": "edict.compiler.settings/v1", "type": "compilerSettings",
        "operation": "build", document: f"edict.{document}.json",
    }
    result = subprocess.run(
        [binary], cwd=root, input=json.dumps(request) + "\n", text=True,
        capture_output=True, timeout=120, check=False,
    )
    events = [json.loads(line) for stream in (result.stdout, result.stderr)
              for line in stream.splitlines()]
    diagnostics = [event for event in events if event.get("type") == "diagnostic"]
    if expected_kind is None:
        valid = result.returncode == 0 and not diagnostics
    else:
        valid = (result.returncode == 2 and len(diagnostics) == 1
                 and diagnostics[0]["kind"] == expected_kind)
    if not valid:
        raise RuntimeError(f"Unexpected public build boundary: {result}\n{events}")
    for diagnostic in diagnostics:
        print(json.dumps(diagnostic, sort_keys=True))


def main():
    if not Path("/.dockerenv").is_file():
        raise RuntimeError("Use the COPY-based consumer witness Dockerfile")
    producer = Path(os.environ.get("EDICT_SOURCE", "/edict"))
    binary = os.environ.get("EDICT_BINARY", "/edict/target/debug/edict")
    source = Path("/consumer-source/edict/replace-range-probes/state-read")
    provider = Path("/echo-source/schemas/edict-provider/package/v1")
    with tempfile.TemporaryDirectory(prefix="ordered-jim-") as scratch:
        root = Path(scratch)
        for document in ("edict.lawpack.json", "edict.application.json"):
            shutil.copyfile(source / document, root / document)
        shutil.copytree(source / "src", root / "src")
        shutil.copytree(provider, root / ".build/echo-provider")
        build(binary, root, "lawpack", None)
        build(binary, root, "application", "TargetLoweringFailed")
        digest_path = root / "vendor/state-probe/manifest.sha256"
        old_digest = digest_path.read_text().strip()
        definition_path = root / "edict.lawpack.json"
        definition = json.loads(definition_path.read_text())
        selection = definition["lawpack"]["targetAdapters"][0]["acceptedTargetIr"]
        selection["id"] = "echo.span-ir/v2"
        # Experimental selection binds the exact schema used by this producer.
        # This does not claim a released Echo v2 provider or target contract.
        schema = (producer / "docs/abi/edict-target-ir.cddl").read_bytes()
        selection["digest"] = "sha256:" + hashlib.sha256(schema).hexdigest()
        definition_path.write_text(json.dumps(definition, indent=2) + "\n")
        build(binary, root, "lawpack", None)
        new_digest = digest_path.read_text().strip()
        authored = root / "src/ReplaceRange.edict"
        text = authored.read_text()
        if old_digest == new_digest or text.count(old_digest) != 1:
            raise RuntimeError("Expected exactly one changed lawpack import")
        authored.write_text(text.replace(old_digest, new_digest))
        build(binary, root, "application", "InvalidProviderInvocation")
        output = root / ".build/application"
        if output.exists() and list(output.rglob("*")):
            raise RuntimeError("Unsupported provider published application artifacts")
    print("ORDERED_PUBLIC_BUILD_REACHED_OLD_PROVIDER_GATE")


if __name__ == "__main__":
    main()
