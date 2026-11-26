import json, os, re, sys
os.chdir(os.path.dirname(os.path.abspath(__file__)))
py, rs = json.load(open("out_py.json")), json.load(open("out_rs.json"))
steps = json.load(open("scenario.json"))
HEX = re.compile(r"^[0-9a-f]{32}$")
def norm(v, ids):
    if isinstance(v, dict):
        out = {}
        for k, x in v.items():
            if k in ("exported_at", "data_dir"): continue
            if k == "created_at" and x != "2026-11-01 00:00:00": x = "<agora>"
            out[k] = norm(x, ids)
        return out
    if isinstance(v, list): return [norm(x, ids) for x in v]
    if isinstance(v, str) and HEX.match(v):
        return ids.setdefault(v, f"<uuid{len(ids)}>")
    return v
def diff(a, b, path="", acc=None):
    acc = [] if acc is None else acc
    if type(a) != type(b) and not (isinstance(a,(int,float)) and isinstance(b,(int,float)) and not isinstance(a,bool) and not isinstance(b,bool)):
        acc.append(f"{path}: tipo {type(a).__name__}={str(a)[:80]} vs {type(b).__name__}={str(b)[:80]}")
    elif isinstance(a, dict):
        for k in set(a) | set(b):
            if k not in a or k not in b: acc.append(f"{path}.{k}: só em {'py' if k in a else 'rs'}")
            else: diff(a[k], b[k], f"{path}.{k}", acc)
    elif isinstance(a, list):
        if len(a) != len(b): acc.append(f"{path}: tamanho {len(a)} vs {len(b)}")
        for i, (x, y) in enumerate(zip(a, b)): diff(x, y, f"{path}[{i}]", acc)
    elif a != b: acc.append(f"{path}: {str(a)[:100]!r} vs {str(b)[:100]!r}")
    return acc
bad = 0
ip, ir = {}, {}
for i, (s, a, b) in enumerate(zip(steps, py, rs)):
    a, b = norm(a, ip), norm(b, ir)
    if isinstance(a, dict) and "__error__" in a and isinstance(b, dict) and "__error__" in b:
        continue  # os dois deram erro (mensagens comparadas à parte)
    d = diff(a, b)
    if d:
        bad += 1
        print(f"#{i} {s['method']} {s['path']}: {len(d)} diferenças"); [print("   ", x) for x in d[:8]]
print(f"\n{len(steps)} passos, {bad} com diferença")
for i,(a,b) in enumerate(zip(py,rs)):
    if (isinstance(a,dict) and "__error__" in a) or (isinstance(b,dict) and "__error__" in b):
        print("erro", i, "|", a.get("__error__") if isinstance(a,dict) else a, "|", b.get("__error__") if isinstance(b,dict) else b)
