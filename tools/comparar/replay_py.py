# Repete scenario.json na versão Python original e grava as respostas.
import os, sys, json, tempfile, datetime as dt, importlib.machinery, importlib.util
HERE = os.path.dirname(os.path.abspath(__file__))
from urllib.parse import parse_qs, urlparse
work = tempfile.mkdtemp()
os.environ["XDG_DATA_HOME"] = work
os.environ["FINANCAS_DB"] = os.path.join(work, "financas.db")
APP = os.path.abspath(os.environ.get("FINANCAS_PY_APP", os.path.join(os.path.dirname(__file__), "..", "..", "..", "app")))
sys.path.insert(0, APP)
import db
T = dt.date(2026, 11, 25)
db.today = lambda: T
import server
import notifier
captured = []
notifier.notify = lambda title, message, url=None, kind="info", actions=None, push=True: captured.append(
    {"title": title, "message": message, "kind": kind, "actions": [[a, r] for a, r in (actions or [])]})
loader = importlib.machinery.SourceFileLoader("financas", APP + "/financas.pyw")
spec = importlib.util.spec_from_loader("financas", loader)
fin = importlib.util.module_from_spec(spec); loader.exec_module(fin)
fin.notifier.notify = notifier.notify

seed = json.load(open(APP + "/dados_iniciais.json", encoding="utf-8"))
db.import_all(seed); db.save_settings({"seed_version": "2"})

def sub(v, vars_):
    if isinstance(v, str) and v.startswith("@"): return vars_[v[1:]]
    if isinstance(v, dict): return {k: sub(x, vars_) for k, x in v.items()}
    return v

vars_, out, last_export = {}, [], None
for st in json.load(open(os.path.join(HERE, "scenario.json"))):
    m, path = st["method"], st["path"]
    for k, v in vars_.items(): path = path.replace("@" + k, str(v))
    body = sub(st.get("body"), vars_)
    try:
        if m == "REMIND":
            captured.clear()
            fin.check_reminders(dt.datetime.fromisoformat(path), st.get("force"))
            res = list(captured)
        elif m == "POSTBACKUP":
            db.import_all(json.loads(json.dumps(last_export))); res = {"ok": True}
        else:
            u = urlparse("/api/" + path)
            q = {k: v[0] for k, v in parse_qs(u.query).items()}
            parts = u.path[5:].strip("/").split("/")
            h = server.Handler.__new__(server.Handler)
            h._body = lambda b=body: json.loads(json.dumps(b)) if b is not None else {}
            res = h.api(m, parts, q)
            if res is None: res = {"ok": True}
            res = json.loads(json.dumps(res, ensure_ascii=False, default=str))
            if path == "backup" and m == "GET": last_export = res
    except Exception as e:
        res = {"__error__": type(e).__name__ + ": " + str(e)}
    if st.get("save") and isinstance(res, dict) and "id" in res: vars_[st["save"]] = res["id"]
    out.append(res)
json.dump(out, open(os.path.abspath(sys.argv[1]) if False else sys.argv[1], "w"), ensure_ascii=False)
print("ok", len(out))
