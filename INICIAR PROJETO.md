# Iniciar o projeto Olhorganizer

Guia com todos os comandos para preparar o ambiente, criar e rodar o projeto (Rust + Tauri 2 + Svelte + TypeScript).

---

## 1. Dependências do sistema

Use **apenas a seção da sua distribuição**.

### Zorin OS / Ubuntu / Mint

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev git
```

### Fedora

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel git
sudo dnf group install "c-development"
```

---

## 2. Rust

Instala na pasta do usuário, sem `sudo`. Escolha a opção **1** (padrão) e depois **feche e reabra o terminal**.

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
rustc --version
cargo --version
```

Para atualizar o Rust no futuro:

```bash
rustup update
```

---

## 3. Node.js

### Zorin OS / Ubuntu (via nvm — o Node do repositório costuma ser antigo)

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
# feche e reabra o terminal
nvm install --lts
node -v
npm -v
```

### Fedora

```bash
sudo dnf install nodejs
node -v
npm -v
```

---

## 4. Extensões do VSCodium

Instale pelo painel de extensões (Open VSX):

- **rust-analyzer** — autocompletar e erros de Rust
- **Svelte for VS Code** — arquivos `.svelte`
- **Tauri** (tauri-apps) — ajuda no `tauri.conf.json`
- **Even Better TOML** — arquivos `Cargo.toml`
- **CodeLLDB** — depurar Rust com pontos de parada

> **Zorin:** se o VSCodium foi instalado pela Loja de Software (Flatpak), ele pode não enxergar o `cargo` e o `node`. Verifique com:
>
> ```bash
> flatpak list | grep -i codium
> ```
>
> Se aparecer, prefira instalar o VSCodium pelo `.deb` oficial.

---

## 5. Criar o projeto

```bash
cd ~/Projetos
npm create tauri-app@latest
```

Respostas do assistente:

| Pergunta | Resposta |
| --- | --- |
| Project name | `Olhorganizer` |
| Identifier | `com.diogosperandio.olhorganizer` (sem ponto no início) |
| Frontend language | `TypeScript / JavaScript` |
| Package manager | `npm` |
| UI template | `Svelte` |
| UI flavor | `TypeScript` |

---

## 6. Rodar pela primeira vez

```bash
cd Olhorganizer
npm install
npm run tauri dev
```

- A primeira compilação demora alguns minutos.
- Deve abrir uma janela com os logos do Tauri, Vite e Svelte.
- Para parar: feche a janela ou `Ctrl+C` no terminal.
- Com o `tauri dev` rodando, mudanças em `.svelte` aparecem na hora; mudanças em Rust recompilam e reabrem o app.

---

## 7. Corrigir o identifier

Abra o projeto no VSCodium:

```bash
codium .
```

Em `src-tauri/tauri.conf.json`, confira a linha:

```json
"identifier": "com.diogosperandio.olhorganizer",
```

---

## 8. Versionar com Git

```bash
git init
git add .
git commit -m "Projeto inicial"
```

Depois de criar o repositório vazio no GitHub:

```bash
git remote add origin https://github.com/<seu-usuario>/olhorganizer.git
git branch -M main
git push -u origin main
```

---

## 9. Criar o motor como biblioteca separada

```bash
cargo new --lib crates/motor
```

Crie o arquivo **`Cargo.toml` na raiz** do projeto:

```toml
[workspace]
members = ["src-tauri", "crates/motor"]
resolver = "2"
```

Em **`src-tauri/Cargo.toml`**, na seção `[dependencies]`, adicione:

```toml
motor = { path = "../crates/motor" }
```

Adicione as dependências do motor:

```bash
cd crates/motor
cargo add serde --features derive
cargo add serde_json regex chrono
cd ../..
```

Confirme que tudo compila:

```bash
npm run tauri dev
```

Estrutura esperada:

```
Olhorganizer/
├── Cargo.toml          # workspace
├── crates/motor/       # regras, marcadores, mover/copiar + testes
├── src-tauri/          # app Tauri: monitor, bandeja, comandos
└── src/                # interface Svelte + TypeScript
```

---

## 10. Primeiro pedido ao Claude (fase 1 do roteiro)

Com a pasta do projeto aberta no VSCodium:

> Implemente em `crates/motor` uma struct `Regra` (pasta de origem, filtro, destino, ação mover/copiar) com serde, a conversão de um filtro com marcadores como `{YYYY}{MM}{DD}.pdf` em regex, e a montagem do caminho de destino a partir de `{YYYY}/{MM}`. Inclua testes com `cargo test` para nome válido, data inválida e arquivo que não casa com o filtro.

Rodar os testes do motor:

```bash
cargo test -p motor
```

---

## 11. Comandos do dia a dia

| Ação | Comando |
| --- | --- |
| Rodar em desenvolvimento | `npm run tauri dev` |
| Testar o motor | `cargo test -p motor` |
| Testar tudo | `cargo test --workspace` |
| Verificar erros sem compilar tudo | `cargo check --workspace` |
| Formatar o código Rust | `cargo fmt --all` |
| Sugestões de melhoria no Rust | `cargo clippy --workspace` |
| Gerar instaladores | `npm run tauri build` |
| Adicionar um plugin do Tauri | `npm run tauri add <plugin>` |

Os instaladores gerados ficam em `src-tauri/target/release/bundle/`.

---

## 12. Mais tarde (fases 5 e 6)

Plugins da bandeja, sistema e atualização:

```bash
npm run tauri add autostart
npm run tauri add notification
npm run tauri add dialog
npm run tauri add single-instance
npm run tauri add updater
npm run tauri add process
```

Chave de assinatura das atualizações automáticas (gerar **uma vez** e guardar bem):

```bash
npm run tauri signer generate -- -w ~/.tauri/olhorganizer.key
```

- A chave **pública** vai no `tauri.conf.json` (`plugins.updater.pubkey`).
- A chave **privada** e a senha vão nos segredos do GitHub: `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Nunca faça commit do arquivo `.key`.

Se a geração do AppImage falhar:

```bash
sudo apt install libfuse2      # Zorin / Ubuntu
sudo dnf install fuse-libs     # Fedora
```

---

Referência: https://v2.tauri.app/start/prerequisites/
