#!/usr/bin/env bash
# Compara a versão Rust com a versão Python original, chamada por chamada (96 passos:
# leituras, lançamentos, parcelas, contas fixas, faturas, transferências, backup, lembretes).
# Uso: FINANCAS_PY_APP=/caminho/da/pasta/app bash tools/comparar/comparar.sh
set -e
cd "$(dirname "$0")"
python3 -I scenario.py
python3 -I replay_py.py "$PWD/out_py.json"
cargo run -q -p financas-core --example replay -- "$PWD/scenario.json" "$PWD/out_rs.json"
python3 -I compare.py
