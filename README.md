# Finanças Local 2 (Tauri)

O mesmo Finanças, agora como app nativo: um instalador só no Windows, pacote para o Ubuntu,
sem Python e sem abrir o Edge. O painel da bandeja e os avisos usam o mesmo HTML/CSS do app.

## O que muda para você

- **Os dados são os mesmos.** O app abre o mesmo banco, no mesmo lugar:
  `%APPDATA%\FinancasLocal\financas.db` no Windows e `~/.local/share/financas-local/` no Ubuntu.
  Nada é importado de novo e nada é perdido. O backup diário continua na pasta `backups`, guardando os 15 últimos.
- **Mesmas regras, mesmas telas.** Faturas, parcelas, contas fixas, saldos e lembretes foram portados
  do `db.py` para Rust e conferidos chamada por chamada contra a versão Python (veja "Conferência" abaixo).
- **Novidades:**
  - janela sem a moldura do navegador, com barra de título própria;
  - painel da bandeja e avisos com a faixa esmeralda do app;
  - transição suave entre as telas;
  - opção "Iniciar com o computador" em Configurações → Lembretes e avisos;
  - Enter num formulário agora salva (antes acionava o "Cancelar").
- Fechar a janela só a esconde: o app continua na bandeja com os lembretes. Para encerrar, use **Sair**.

## Trocar a versão Python pela nova (Windows)

Faça nesta ordem. O desinstalador antigo apaga os atalhos chamados "Finanças", e o novo instalador cria atalhos com esse mesmo nome.

1. Na versão antiga: **Configurações → Exportar backup** (por garantia).
2. Rode o `desinstalar.bat` antigo e aperte **Enter** quando ele perguntar sobre os dados, para mantê-los.
3. Rode `Finanças_2.0.0_x64-setup.exe` — ou dê dois cliques em `windows/instalar-v2.bat`, que faz os passos 2 e 3 sozinho
   (acha o instalador em Downloads, inclusive dentro do `financas-windows.zip`, remove a versão Python mantendo os dados,
   copia o banco para `backups` e instala).
   - O Windows pode mostrar "O Windows protegeu o computador", porque o instalador não tem assinatura digital paga.
   - Clique em **Mais informações → Executar assim mesmo**.
4. O app abre com os seus lançamentos de sempre e o ícone aparece perto do relógio.

Se a versão antiga ainda estiver aberta, o app novo avisa: os lembretes chegariam em dobro.

## Ubuntu

- **Pacote .deb:** `sudo apt install ./financas_2.0.0_amd64.deb`
- **AppImage:** dê permissão de execução e abra; não precisa instalar.
- O ícone da bandeja precisa da extensão AppIndicator, que já vem ligada no Ubuntu.
- O clique no ícone abre um menu; **Abrir painel** é o primeiro item.
  - Para abrir o painel por um atalho de teclado, aponte o atalho para `financas-local --painel`.
- **Tremor da tela:** na sessão padrão (Wayland), o sistema não deixa programas fotografarem a tela, então só o aviso treme.
  - Na sessão "Ubuntu em Xorg", a tela inteira treme.
- **Vindo do Windows:**
  1. No Windows: Configurações → Exportar backup.
  2. No Ubuntu: Configurações → Importar backup.

## Como gerar os instaladores

### Pelo GitHub (recomendado, não precisa instalar nada no PC)

1. Repositório: https://github.com/fernanduandrade/financas-desktop (privado).
2. A cada push, o GitHub Actions roda os testes e gera:
   - `financas-windows` (o `.exe` de instalação);
   - `financas-ubuntu` (`.deb` e `.AppImage`).
3. Baixe os arquivos na aba **Actions**, na execução mais recente, em "Artifacts".
4. Para uma versão "oficial", crie uma tag `v2.0.0`. O workflow anexa os arquivos a um rascunho de Release.

### No próprio PC

Precisa de Rust (https://rustup.rs) e Node 20 ou mais novo. No Windows, também do Visual Studio Build Tools, com "Desenvolvimento para desktop com C++" (uns 5–6 GB).

```
npm install
npm run tauri build
```

No Ubuntu, antes:

```
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
  libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev libgbm-dev
```

Os instaladores saem em `target/release/bundle/`.

## Conferência

- `cargo test -p financas-core`: confere os valores de 25/11/2026 a partir de `dados_iniciais.json`.
  - Saldos: Itaú R$ 34,60, Vale R$ 1,30, Casa R$ 0,00, Mercado Pago R$ 16,00.
  - Disponível nos cartões: R$ 14.228,62, R$ 6.530,41 e R$ 3.465,29.
  - Faturas a pagar: R$ 9.437,72, com a fatura fechada do Platinum de R$ 3.384,92.
  - Testa também a regra do fechamento e o backup de ida e volta.
- `tools/comparar/comparar.sh`: roda a mesma sequência de 96 chamadas na versão Python original e na versão Rust e compara as respostas.
  - A sequência cobre leituras, lançamentos, parcelas, contas fixas, faturas, transferências, contas, categorias, configurações, backup e lembretes.
  - Precisa da pasta `app/` antiga: `FINANCAS_PY_APP=/caminho/app bash tools/comparar/comparar.sh`.
  - Resultado atual: **0 diferenças**.

## Estrutura

| Pasta | O que tem |
|---|---|
| `core/` | Banco SQLite e todas as regras (porta do `db.py`), roteador da API (rotas do `server.py`) e lembretes (do `financas.pyw`). Não depende do Tauri. |
| `core/seed/` | `dados_iniciais.json` do Organizze, embutido no executável para a primeira execução. |
| `src-tauri/` | O app: janelas, bandeja, agendador (a cada 30 s), avisos, tremor, ntfy, início com o sistema e instância única. |
| `ui/` | Frontend: `app.js`, `style.css` e `index.html` da versão antiga, mais `panel.*`, `toast.*`, `shake.*` e `mini.css`. |
| `tools/comparar/` | Comparação com a versão Python. |

O `app.js` continua chamando `api(method, path, body)`. No app, isso vira `invoke('api', …)` e cai no roteador em Rust.
Aberto num navegador comum, ele volta a usar o `fetch` de antes.

## Próximos passos possíveis

- Efeito Mica/Acrylic no Windows 11 (precisa testar num PC com Windows 11).
- Modo escuro.
- Assinatura digital do instalador, para o Windows não mostrar o alerta.
