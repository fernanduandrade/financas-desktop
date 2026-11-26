# Gera a sequência de chamadas usada na comparação Python x Rust.
import json, os
os.chdir(os.path.dirname(os.path.abspath(__file__)))
S = []
def c(method, path, body=None, save=None, force=None):
    st = {"method": method, "path": path}
    if body is not None: st["body"] = body
    if save: st["save"] = save
    if force: st["force"] = force
    S.append(st)

# leituras do estado importado
c("GET", "summary"); c("GET", "meta"); c("GET", "recurrences")
for card in (4, 5, 6, 8):
    c("GET", f"invoices?card={card}")
for card, due in ((4, "2026-12-02"), (5, "2026-12-03"), (6, "2026-12-01"), (6, "2027-01-01"), (4, "2026-11-02")):
    c("GET", f"invoices?card={card}&due={due}")
for m in ("2025-11", "2026-03", "2026-10", "2026-11", "2026-12", "2027-01"):
    c("GET", f"transactions?start={m}-01&end={m}-31")
c("GET", "transactions?start=2026-01-01&end=2026-12-31&account=1")
c("GET", "transactions?start=2026-01-01&end=2026-12-31&category=1")
c("GET", "transactions?start=2025-01-01&end=2027-12-31&q=uber")
c("GET", "transactions?start=2025-01-01&end=2027-12-31&q=Pagamento%20fatura")
c("GET", "transactions?start=2026-11-01&end=2026-11-30&account=&category=&q=")
c("GET", "transactions?end=2026-11-30")                       # erro: falta start
c("GET", "settings")
c("REMIND", "2026-11-25T08:00:00")                            # antes de tudo: nada
c("REMIND", "2026-11-25T21:30:00")                            # contas + diário
c("REMIND", "2026-11-25T22:00:00")                            # já avisou hoje
c("REMIND", "2026-11-29T19:30:00")                            # domingo: contas, diário, semanal
c("REMIND", "2026-11-25T10:00:00", force="contas")
c("REMIND", "2026-11-25T10:00:00", force="semanal")

# lançamentos
c("POST", "transactions", {"description": "Mercado", "amount_cents": -12345, "date": "2026-11-25", "account_id": 1, "category_id": 1, "notes": None, "paid": 1}, save="t1")
c("POST", "transactions", {"description": "Farmácia", "amount_cents": -4590, "date": "2026-11-25", "account_id": 4, "category_id": 2, "paid": 1}, save="t2")
c("POST", "transactions", {"description": "Compra pós fechamento", "amount_cents": -9900, "date": "2026-11-26", "account_id": 4, "category_id": 2, "paid": 1})
c("POST", "transactions", {"description": "Celular 10x", "amount_cents": -199999, "date": "2026-11-24", "account_id": 5, "category_id": 3, "paid": 1, "parcelas": 10}, save="t3")
c("POST", "transactions", {"description": "Antecipada", "amount_cents": -30000, "date": "2027-02-10", "account_id": 6, "category_id": 3, "paid": 1, "invoice_due": "2026-12-01", "parcelas": 3})
c("POST", "transactions", {"description": "Curso 4x conta", "amount_cents": -100001, "date": "2026-01-31", "account_id": 1, "category_id": 4, "paid": 1, "parcelas": "4"}, save="t4")
c("POST", "transactions", {"description": "Salário extra", "amount_cents": 50000, "date": "2026-11-30", "account_id": 1, "category_id": 45, "paid": 0})
c("POST", "transactions", {"description": "Streaming", "amount_cents": -3990, "date": "2026-11-12", "account_id": 4, "category_id": 5, "paid": 1, "fixa": True}, save="t5")
c("POST", "transactions", {"description": "Seguro", "amount_cents": -25000, "date": "2026-10-31", "account_id": 1, "category_id": 5, "paid": 1, "fixa": True}, save="t6")
c("GET", "summary"); c("GET", "recurrences")
c("GET", "transactions?start=2026-11-01&end=2027-03-31")
# edições
c("PUT", "transactions/@t2", {"date": "2026-11-27", "description": "Farmácia (data nova)", "amount_cents": -4590, "account_id": 4, "category_id": 2, "notes": None, "paid": 1})
c("PUT", "transactions/@t2?scope=one", {"date": "2026-11-27", "account_id": 4, "invoice_due": "2026-12-02"})
c("PUT", "transactions/@t2", {"account_id": 1})
c("PUT", "transactions/@t3?scope=future", {"description": "Celular (renomeado)", "amount_cents": -20000, "category_id": 4})
c("PUT", "transactions/@t5?scope=future", {"amount_cents": -4590, "description": "Streaming+"})
c("PUT", "transactions/@t1", {"paid": 0})
c("PUT", "transactions/@t1", {"paid": True})
c("PUT", "transactions/99999", {"paid": 1})
c("GET", "recurrences"); c("GET", "summary")
# transferências
c("POST", "transfers", {"from_id": 1, "to_id": 2, "amount_cents": 15000, "date": "2026-11-25", "description": ""}, save="tr")
c("POST", "transfers", {"from_id": 1, "to_id": 1, "amount_cents": 15000, "date": "2026-11-25"})
c("GET", "transactions?start=2026-11-25&end=2026-11-25")
# faturas
c("POST", "invoices/pay", {"card_id": 6, "due": "2026-12-01", "account_id": 1, "date": "2026-11-25", "amount_cents": None}, save="pay")
c("POST", "invoices/pay", {"card_id": 4, "due": "2026-12-02", "account_id": 1, "date": "2026-11-25", "amount_cents": 100000})
c("GET", "invoices?card=6"); c("GET", "summary")
c("POST", "invoices/unpay", {"card_id": 4, "due": "2026-12-02"})
c("DELETE", "transactions/@pay")
c("GET", "invoices?card=6"); c("GET", "invoices?card=4")
# exclusões
c("DELETE", "transactions/@t4?scope=future")
c("DELETE", "transactions/@t6?scope=future")
c("DELETE", "transactions/@t3?scope=one")
c("GET", "recurrences")
# contas fixas
c("POST", "recurrences", {"description": "Gás", "amount_cents": -9000, "account_id": 3, "category_id": 6, "day": 20}, save="r1")
c("POST", "recurrences", {"description": "Freela", "amount_cents": 80000, "account_id": 1, "category_id": 46, "day": 31}, save="r2")
c("PUT", "recurrences/@r1", {"description": "Gás encanado", "amount_cents": -9500, "account_id": 3, "category_id": 6, "day": 20})
c("DELETE", "recurrences/@r2?future=0")
c("DELETE", "recurrences/1?future=1")
c("GET", "recurrences")
# contas e cartões
c("POST", "accounts", {"name": "Poupança", "kind": "conta", "color": "#123456", "archived": 0, "balance_cents": 123456}, save="a1")
c("PUT", "accounts/1", {"name": "Itaú - Principal", "kind": "conta", "color": "#EC7000", "archived": 0, "balance_cents": 10000})
c("POST", "accounts", {"name": "Cartão novo", "kind": "cartao", "color": "#222222", "archived": 0, "limit_cents": 500000, "closing_day": 5, "due_day": 12, "payment_account_id": 1}, save="a2")
c("POST", "transactions", {"description": "Teste cartão novo", "amount_cents": -1000, "date": "2026-11-05", "account_id": "@a2", "paid": 1})
c("DELETE", "accounts/@a1"); c("DELETE", "accounts/@a2")
c("GET", "meta")
# categorias
c("POST", "categories", {"name": "Nova", "kind": "despesa", "parent_id": None, "color": "#abcdef"}, save="c1")
c("POST", "categories", {"name": "Sub", "kind": "despesa", "parent_id": "@c1", "color": "#abcdef"}, save="c2")
c("PUT", "categories/@c2", {"name": "Sub renomeada", "kind": "despesa", "parent_id": "@c1", "color": "#000000"})
c("DELETE", "categories/@c1"); c("DELETE", "categories/1")
c("GET", "meta")
# configurações
c("POST", "settings", {"lembrete_diario": "20:30", "som_aviso": "0", "aviso_contas_dias": "5", "x_bool": True, "x_none": None, "x_num": 3})
c("GET", "settings")
c("REMIND", "2026-11-26T09:30:00")
# backup ida e volta
c("GET", "backup", save_export=True) if False else c("GET", "backup")
c("POSTBACKUP", "backup")                                      # importa o próprio export
c("POST", "backup", {"app": "outro"})
c("GET", "summary"); c("GET", "backup")
c("GET", "rota/que/nao/existe")
json.dump(S, open("scenario.json", "w"), ensure_ascii=False, indent=0)
print(len(S), "passos")
