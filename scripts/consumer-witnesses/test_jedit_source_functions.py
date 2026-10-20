"""Run in the guarded Docker worker: python3 -B scripts/consumer-witnesses/test_jedit_source_functions.py."""
import importlib.util
from pathlib import Path
import tempfile
import time
import unittest


class ChildCleanupTest(unittest.TestCase):
    def test_successful_compiler_exit_stops_its_surviving_descendant(self):
        self.assertTrue(Path("/.dockerenv").is_file(), "requires guarded Docker worker")
        spec = importlib.util.spec_from_file_location(
            "consumer_witness", Path(__file__).with_name("jedit-source-functions.py"))
        witness = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(witness)
        with tempfile.TemporaryDirectory(prefix="edict-child-cleanup-") as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/RangeAssembly.edict").write_text("test input")
            compiler = root / "compiler.py"
            compiler.write_text("""#!/usr/bin/env python3
import json, os, signal, sys, time
from pathlib import Path
sys.stdin.readline()
reader, writer = os.pipe()
pid = os.fork()
if pid == 0:
    os.close(reader)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    Path('descendant.pid').write_text(str(os.getpid()))
    os.write(writer, b'ready')
    os.close(writer)
    time.sleep(10)
    os._exit(0)
os.close(writer)
assert os.read(reader, 5) == b'ready'
os.close(reader)
print(json.dumps({'type': 'status', 'exitCode': 0}), flush=True)
""")
            compiler.chmod(0o700)
            result = witness.build(compiler, root)
            self.assertEqual(result["exitCode"], 0)
            pid = int((root / "descendant.pid").read_text())
            # A killed orphan may briefly remain a zombie under container PID 1.
            def running() -> bool:
                try:
                    state = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1][0]
                except FileNotFoundError:
                    return False
                return state not in {"Z", "X"}
            deadline = time.monotonic() + 1
            while running() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertFalse(running(), "compiler descendant survived successful build")


if __name__ == "__main__":
    unittest.main()
