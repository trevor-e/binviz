"""Drive the decompilation loop through binviz's MCP server, the way an
agent would, with the compiler's own objects standing in for the C the
agent writes: Map (open, name), Pick (next_functions), Context
(decomp_context, similar_functions), Compile, Compare (match_function,
mark). Every tool call goes over the same JSON-RPC the agent uses.

    python samples/loop.py samples/psx-vm/build --rounds 40 [--mcp target/release/binviz-mcp.exe]

The build folder holds main.exe, the objects (*.o), candidates.json,
library.json and main.notes.json (the ground truth, used only to pick the
object function that "the agent's C" compiles to, and to check names).
"""
import argparse
import glob
import json
import os
import re
import subprocess
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


class Mcp:
    def __init__(self, exe):
        self.p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, encoding="utf-8")
        self.n = 0
        self.rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "loop", "version": "0"}})
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        self.p.stdin.flush()

    def rpc(self, method, params):
        self.n += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise SystemExit("the MCP server exited")
            msg = json.loads(line)
            if msg.get("id") == self.n:
                return msg

    def call(self, tool, **args):
        msg = self.rpc("tools/call", {"name": tool, "arguments": args})
        if "error" in msg:
            return "ERROR: " + msg["error"].get("message", str(msg["error"]))
        r = msg["result"]
        text = "\n".join(c.get("text", "") for c in r.get("content", []))
        if r.get("isError"):
            return "ERROR: " + text
        return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("build")
    ap.add_argument("--rounds", type=int, default=30)
    ap.add_argument("--mcp", default=os.path.join("target", "release", "binviz-mcp.exe" if os.name == "nt" else "binviz-mcp"))
    ap.add_argument("--fail-every", type=int, default=4, help="every Nth pick, the first try 'doesn't match' (another function's code is compared)")
    args = ap.parse_args()
    build = os.path.abspath(args.build)
    exe = os.path.join(build, "main.exe")
    truth = {a["address"]: a["name"] for a in json.load(open(os.path.join(build, "main.notes.json"))) if a.get("kind") == "function"}
    library = set(json.load(open(os.path.join(build, "library.json"))))
    objects = sorted(glob.glob(os.path.join(build, "*.o")))
    notes = os.path.join(build, "loop.notes.json")
    if os.path.exists(notes):
        os.remove(notes)
    mcp = Mcp(args.mcp)
    log = lambda *a: print(*a, flush=True)

    log("== Map")
    log(mcp.call("open_binary", path=exe, notes_file=notes).splitlines()[0])
    out = mcp.call("propose_names", path=os.path.join(build, "candidates.json"), apply=True, min_confidence=0.6)
    log(out.splitlines()[0], "|", out.splitlines()[-1])
    proposed = {int(m.group(1), 16): m.group(2) for m in re.finditer(r"^(0x[0-9a-f]+)\s+\S+\s+->\s+(\S+)", out, re.M)}
    right = sum(1 for a, n in proposed.items() if truth.get(a, "").replace("_", "").lower() == n.lower())
    log(f"names proposed: {len(proposed)}, right: {right}, wrong: {len(proposed) - right}")
    for a, n in proposed.items():
        if truth.get(a, "").replace("_", "").lower() != n.lower():
            log(f"  wrong: {a:#x} {truth.get(a)} -> {n}")
    # The runtime is library code (what identify_sdk would say with the SDK's libraries).
    lib_marked = 0
    for a, n in truth.items():
        if n in library:
            mcp.call("mark", at=f"{a:#x}", state="library", source="rt")
            lib_marked += 1
    log(f"library functions marked: {lib_marked}")

    log("\n== The loop")
    failed_once = set()
    matched = 0
    for round_ in range(1, args.rounds + 1):
        pick = mcp.call("next_functions", count=3, claim=True, agent="loop")
        m = re.search(r"^\s*1\.\s+\S+ \((0x[0-9a-f]+)\).*$", pick, re.M)
        if not m:
            log(pick)
            log("nothing left to pick")
            break
        at = int(m.group(1), 16)
        why = m.group(0).strip()[3:]
        progress = next((l for l in pick.splitlines() if "%" in l), "")
        log(f"\n[{round_}] {progress}")
        log(f"  pick: {why}")
        ctx = mcp.call("decomp_context", at=f"{at:#x}", limit=60)
        sig = next((l for l in ctx.splitlines() if "(" in l and l.rstrip().endswith(")")), "?")
        log(f"  context: {sig}; {len(ctx.splitlines())} lines")
        sim = mcp.call("similar_functions", at=f"{at:#x}", count=3)
        log("  similar: " + " / ".join(l.strip() for l in sim.splitlines()[1:4]))
        name = truth.get(at)
        if name is None:
            log("  not a function of the build (padding, data?): skipped")
            mcp.call("mark", at=f"{at:#x}", state="skipped")
            continue
        # "Compile": the object holding this function; a wrong function's code on a first try every Nth pick.
        symbol = name
        if args.fail_every and round_ % args.fail_every == 0 and at not in failed_once:
            others = [n for n in truth.values() if n != name and n not in library]
            symbol = others[round_ % len(others)]
            failed_once.add(at)
        result = None
        for obj in objects:
            out = mcp.call("match_function", object=obj, symbol=symbol, at=f"{at:#x}")
            if not out.startswith("ERROR"):
                result = (obj, out)
                break
        if result is None:
            log(f"  no object holds {symbol}")
            mcp.call("mark", at=f"{at:#x}", state="skipped")
            continue
        obj, out = result
        pm = re.search(r"(\d+(?:\.\d+)?)%", out)
        percent = float(pm.group(1)) if pm else 0.0
        kinds = [l.strip() for l in out.splitlines()[1:6] if "×" in l]
        log(f"  compare: {os.path.basename(obj)}:{symbol} -> {percent:.1f}%" + (f"  ({'; '.join(kinds)})" if kinds else ""))
        if percent >= 100.0:
            mcp.call("annotate", at=f"{at:#x}", name=name)
            out = mcp.call("mark", at=f"{at:#x}", state="matched", percent=100, source=f"src/{os.path.basename(obj)[:-2]}.c")
            matched += 1
            log("  mark: " + out.splitlines()[0])
        else:
            out = mcp.call("mark", at=f"{at:#x}", state="attempted", percent=percent)
            log("  mark: " + out.splitlines()[0])

    log("\n== Where it stands")
    final = mcp.call("next_functions", count=5)
    log("\n".join(final.splitlines()[:8]))
    log(f"\nmatched {matched} functions in {round_} rounds; notes in {notes}")


if __name__ == "__main__":
    main()
