#!/usr/bin/env bash
# Prepares a version, commits it, tags it and starts the GitHub release workflow.
#
# Usage: scripts/release.sh [VERSION] [--skip-checks] [--dry-run]
#   VERSION        optional X.Y.Z version; when omitted, an interactive prompt is shown
#   --skip-checks  skips local formatting, lint, test and frontend checks
#   --dry-run      displays the planned commands without changing files or pushing
set -euo pipefail

skip_checks=0
dry_run=0
requested_version=""

for arg in "$@"; do
  case "$arg" in
    --skip-checks) skip_checks=1 ;;
    --dry-run) dry_run=1 ;;
    -h|--help)
      sed -n '2,7p' "$0"
      exit 0
      ;;
    -* )
      echo "Opção desconhecida: $arg (use --help)." >&2
      exit 1
      ;;
    *)
      if [[ -n "$requested_version" ]]; then
        echo "Informe somente uma versão." >&2
        exit 1
      fi
      requested_version="${arg#v}"
      ;;
  esac
done

red=$'\033[31m'
green=$'\033[32m'
yellow=$'\033[33m'
bold=$'\033[1m'
reset=$'\033[0m'

info() { echo "${bold}$*${reset}"; }
ok() { echo "${green}✓ $*${reset}"; }
warn() { echo "${yellow}! $*${reset}"; }
die() { echo "${red}✗ $*${reset}" >&2; exit 1; }

run() {
  if [[ "$dry_run" -eq 1 ]]; then
    printf '[dry-run]'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

ask_yes_no() {
  local question="$1"
  local default_answer="${2:-n}"
  local hint answer

  if [[ "$default_answer" == "s" ]]; then
    hint="[S/n]"
  else
    hint="[s/N]"
  fi

  read -r -p "$question $hint " answer || answer=""
  answer="${answer:-$default_answer}"
  [[ "$answer" =~ ^[sSyY]$ ]]
}

project_dir="$(git rev-parse --show-toplevel 2>/dev/null)" \
  || die "Execute este script dentro do repositório Git."
cd "$project_dir"

[[ -f package.json && -f src-tauri/tauri.conf.json && -f scripts/bump-version.sh ]] \
  || die "Os arquivos do Olhorganizer não foram encontrados na raiz do repositório."
git remote get-url origin >/dev/null 2>&1 \
  || die "O remote 'origin' não está configurado."

branch="$(git branch --show-current)"
[[ -n "$branch" ]] || die "HEAD destacado: entre em uma branch antes de lançar."
if [[ "$branch" != "main" && "$branch" != "master" ]]; then
  warn "Você está na branch '$branch' (não é main/master)."
  ask_yes_no "Continuar mesmo assim?" n || exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  git status --short
  die "Há alterações não commitadas. Faça commit ou stash antes de lançar."
fi

info "Sincronizando tags com o GitHub..."
git fetch --tags origin >/dev/null 2>&1 \
  || die "Falha ao consultar o GitHub. Verifique a conexão."

if upstream="$(git rev-parse --abbrev-ref '@{u}' 2>/dev/null)"; then
  behind="$(git rev-list --count "HEAD..$upstream")"
  [[ "$behind" -eq 0 ]] \
    || die "Sua branch está $behind commit(s) atrás de $upstream. Execute git pull."
fi

semver_tag='^v[0-9]+\.[0-9]+\.[0-9]+$'
last_tag="$(git tag -l 'v*' --sort=-v:refname | grep -E "$semver_tag" | head -n 1 || true)"

current_versions="$(node <<'NODE'
const fs = require("node:fs");

const packageVersion = JSON.parse(fs.readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargo.match(/\[package\][\s\S]*?\nversion = "([^"]+)"\n/)?.[1];

if (!cargoVersion) process.exit(2);
process.stdout.write(`${packageVersion}\n${tauriVersion}\n${cargoVersion}\n`);
NODE
)" || die "Não foi possível ler as versões atuais do projeto."

mapfile -t versions <<<"$current_versions"
current_version="${versions[0]}"
if [[ "${versions[1]}" != "$current_version" || "${versions[2]}" != "$current_version" ]]; then
  die "As versões de package.json, tauri.conf.json e Cargo.toml não coincidem."
fi

if [[ -n "$last_tag" ]]; then
  IFS=. read -r major minor patch <<<"${last_tag#v}"
  suggested="$major.$minor.$((patch + 1))"
else
  suggested="$current_version"
fi

echo
info "Versões atuais"
echo "  Última tag:       ${last_tag:-nenhuma}"
echo "  Versão do código: $current_version"
echo

if [[ -n "$requested_version" ]]; then
  new_version="$requested_version"
else
  read -r -e -i "$suggested" -p "Nova versão (X.Y.Z): " new_version \
    || die "Entrada cancelada."
  new_version="${new_version#v}"
fi

[[ "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
  || die "Versão inválida: '$new_version'. Use X.Y.Z, por exemplo 0.2.0."

highest_code_version="$(printf '%s\n%s\n' "$current_version" "$new_version" | sort -V | tail -n 1)"
if [[ "$highest_code_version" != "$new_version" ]]; then
  die "A nova versão ($new_version) não pode ser menor que a versão do código ($current_version)."
fi

tag="v$new_version"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null \
  && die "A tag $tag já existe localmente."
git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 \
  && die "A tag $tag já existe no GitHub."

if [[ -n "$last_tag" ]]; then
  highest="$(printf '%s\n%s\n' "${last_tag#v}" "$new_version" | sort -V | tail -n 1)"
  if [[ "$highest" != "$new_version" || "$new_version" == "${last_tag#v}" ]]; then
    die "A nova versão deve ser maior que a última tag ($last_tag)."
  fi
fi

echo
info "Resumo"
echo "  Branch:       $branch"
echo "  Versão atual: $current_version"
echo "  Nova versão:  $tag"
echo "  Ações:        verificar, atualizar versões, criar commit e tag, enviar ambos"
echo "  Depois:       GitHub Actions criará uma release provisória para revisão"
[[ "$dry_run" -eq 1 ]] && warn "Modo --dry-run: nenhum arquivo ou remote será alterado."
echo
ask_yes_no "Confirmar e iniciar a release?" n \
  || { echo "Cancelado. Nada foi alterado."; exit 0; }

if [[ "$skip_checks" -eq 0 ]]; then
  info "Executando verificações locais..."
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  npm run check
  npm run test:ui
  npm run build
  ok "Verificações locais passaram."
else
  warn "Verificações locais ignoradas por --skip-checks."
fi

run scripts/bump-version.sh "$new_version"

if [[ "$dry_run" -eq 0 ]]; then
  changed_files="$(git diff --name-only)"
  expected_files=$'Cargo.lock\npackage-lock.json\npackage.json\nsrc-tauri/Cargo.toml\nsrc-tauri/tauri.conf.json'
  unexpected_files="$(comm -23 \
    <(printf '%s\n' "$changed_files" | sed '/^$/d' | sort) \
    <(printf '%s\n' "$expected_files" | sort))"
  [[ -z "$unexpected_files" ]] \
    || die "O bump alterou arquivos inesperados: $unexpected_files"
fi

run git add Cargo.lock package-lock.json package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json
if [[ "$dry_run" -eq 1 ]]; then
  echo "[dry-run] git commit -m Versão\\ $new_version (somente se houver alterações)"
elif ! git diff --cached --quiet; then
  git commit -m "Versão $new_version"
else
  warn "A versão já era $new_version; a tag será criada no commit atual."
fi
run git tag -a "$tag" -m "Versão $new_version"

if ! run git push origin HEAD "$tag"; then
  warn "O push falhou. O commit e a tag existem apenas localmente."
  echo "  Para tentar novamente: git push origin HEAD $tag"
  echo "  Para remover só a tag: git tag -d $tag"
  exit 1
fi

if [[ "$dry_run" -eq 1 ]]; then
  ok "Simulação concluída; nada foi alterado."
  exit 0
fi

ok "Tag $tag enviada ao GitHub."
repository="$(git remote get-url origin | sed -E 's#^(git@github\.com:|https://github\.com/)##; s#\.git$##')"
echo "  Build:   https://github.com/$repository/actions"
echo "  Release: https://github.com/$repository/releases/tag/$tag"

if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  echo
  if ask_yes_no "Acompanhar o build agora?" s; then
    sleep 6
    run_id="$(gh run list --workflow release.yml --limit 1 --json databaseId \
      -q '.[0].databaseId' 2>/dev/null || true)"
    if [[ -n "$run_id" ]]; then
      gh run watch "$run_id" --exit-status \
        && ok "Build de $tag concluído; revise a release provisória." \
        || warn "O build falhou. Consulte https://github.com/$repository/actions/runs/$run_id"
    else
      warn "A execução do workflow ainda não apareceu. Consulte a página de Actions."
    fi
  fi
fi
