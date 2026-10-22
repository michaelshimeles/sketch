"""Exchange a source file while retaining the exact displaced inode for recovery."""
import ctypes
import hashlib
import json
import os
import pathlib
import sys
import uuid


def file_hash(file):
    return hashlib.sha256(pathlib.Path(file).read_bytes()).hexdigest()


def exchange(a, b):
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        fn = libc.renamex_np
        fn.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        result = fn(os.fsencode(a), os.fsencode(b), 2)  # RENAME_SWAP
    elif sys.platform.startswith("linux"):
        fn = libc.renameat2
        fn.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        result = fn(-100, os.fsencode(a), -100, os.fsencode(b), 2)  # RENAME_EXCHANGE
    else:
        raise RuntimeError("Atomic exchange is unavailable on this platform; retain the patch for handoff.")
    if result:
        raise OSError(ctypes.get_errno(), os.strerror(ctypes.get_errno()))


def main():
    destination, expected, replacement, recovery = sys.argv[1:]
    recovery = pathlib.Path(recovery)
    recovery.mkdir(parents=True, exist_ok=True)
    ident = uuid.uuid4().hex
    backup = recovery / (ident + ".displaced")
    if expected == "absent":
        if replacement == "absent":
            raise RuntimeError("Invalid empty change")
        os.link(replacement, destination)  # O_EXCL semantics, never replaces another writer.
        print(json.dumps({"ok": True, "preimage": None}))
        return
    if replacement == "absent":
        os.rename(destination, backup)  # The exact removed inode remains recoverable.
        if file_hash(backup) != expected:
            try:
                os.link(backup, destination)
            except FileExistsError:
                pass  # A newer writer already owns this path; keep both versions.
            raise RuntimeError(f"Concurrent removal conflict; source versions retained in {recovery}")
        print(json.dumps({"ok": True, "preimage": str(backup)}))
        return
    staged = pathlib.Path(destination).with_name(".design-sync-" + ident)
    os.link(replacement, staged)
    proposed_hash = file_hash(staged)
    try:
        exchange(staged, destination)
        os.rename(staged, backup)
        if file_hash(backup) == expected:
            print(json.dumps({"ok": True, "preimage": str(backup)}))
            return
        # A writer raced the preflight. Restore its displaced inode, retaining every
        # version if another writer races restoration too. Never report this as success.
        restore = backup
        previous_hash = proposed_hash
        for index in range(16):
            os.link(restore, staged)
            exchange(staged, destination)
            displaced = recovery / (ident + f".conflict-{index}")
            os.rename(staged, displaced)
            if file_hash(displaced) == previous_hash:
                break
            previous_hash = file_hash(restore)
            restore = displaced
        raise RuntimeError(f"Concurrent edit conflict; source versions retained in {recovery}")
    finally:
        if staged.exists():
            os.rename(staged, recovery / (ident + ".unfinished"))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
