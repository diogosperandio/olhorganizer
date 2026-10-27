# Publicação de uma versão

As versões são produzidas pelo GitHub Actions para Windows e Linux. O workflow
cria uma release provisória; nenhum instalador é publicado automaticamente sem
uma revisão manual.

## Como publicar

O Branch de desenvolvimento é Codex, logo com o tempo o branch `main` vai se desatualizando. Então temos que trocar o branch ativo para `main` (1), atualizar `main` local com remoto (2). Em seguida copiar (commit) do branch `Codex` para `main` (3). Depois publicar `main` (publish) (4)
``` sh
git checkout main  (1)
git pull origin main (2)
git merge Codex -m "Merging Codex into Main"  (3)
git push origin main  (4)
```
Ainda em Main, executar:
```sh
scripts/release.sh
```

## Fluxo recomendado (automático)

Partindo de uma branch limpa e atualizada, execute (a versão é opcional):

```sh
scripts/release.sh 0.1.0
# ou simplesmente: scripts/release.sh
```

O script confirma a branch e o estado do repositório. Se estiver em uma branch
diferente de `main`/`master`, permite continuar nela, trocar para outra branch
local ou cancelar. Depois sincroniza as tags, sugere a próxima versão patch,
executa formatação, clippy, testes e build, chama `scripts/bump-version.sh`, cria
o commit e a tag, e envia ambos ao GitHub. Se o GitHub CLI (`gh`) estiver
autenticado, também pode acompanhar o workflow.

Para conferir todo o plano sem alterar arquivos nem enviar nada:

```sh
scripts/release.sh 0.1.0 --dry-run
```

O GitHub Actions cria uma release **provisória**. A publicação continua manual,
depois dos testes dos instaladores.

## Fluxo manual

`scripts/bump-version.sh` isoladamente apenas atualiza a versão em
`package.json`, `package-lock.json`, `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml` e `Cargo.lock`. Ele não cria commit, tag nem faz push.

Para fazer o processo manualmente, a ordem correta é:

```sh
scripts/bump-version.sh 0.1.0
cargo check --workspace
npm run build
git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml Cargo.lock
git commit -m "Versão 0.1.0"
git tag -a v0.1.0 -m "Versão 0.1.0"
git push origin HEAD v0.1.0
```

## Revisar a release provisória

No GitHub, abra **Actions > Release** e aguarde os dois jobs terminarem. Depois,
abra **Releases**, selecione a release provisória “Olhorganizer v0.1.0” e confira
se ela contém instaladores `.exe`, `.msi`, `.AppImage`, `.deb` e `.rpm`.

Faça o teste básico dos instaladores antes de publicar: Windows 11 para `.exe`
e `.msi`, Zorin para `.AppImage`/`.deb` e Fedora para `.AppImage`/`.rpm`.

## Publicar

Edite as notas da release se necessário e use **Publish release**. Assinatura de
código dos instaladores será adicionada em uma etapa futura.

## Atualizações automáticas

O workflow usa os segredos `TAURI_SIGNING_PRIVATE_KEY` e
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` para assinar os artefatos do updater. A
chave privada nunca deve ser adicionada ao repositório. Somente a chave pública
fica em `plugins.updater.pubkey`, dentro de `src-tauri/tauri.conf.json`. A chave
configurada corresponde ao identificador `3C22B788625F25EB`.

Ao gerar uma tag, confira também se a release provisória contém `latest.json` e
os arquivos `.sig`. O `latest.json` só fica disponível em
`releases/latest/download` depois que a release é publicada. Windows e AppImage
podem atualizar dentro do aplicativo; instalações DEB/RPM abrem a página da
release para que o usuário escolha o pacote do sistema.
