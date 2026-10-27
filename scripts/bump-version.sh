#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Uso: scripts/bump-version.sh VERSÃO" >&2
  echo "Exemplo: scripts/bump-version.sh 0.2.0" >&2
  exit 1
fi

version="${1#v}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  echo "Versão inválida: '$1'. Use o formato 0.2.0." >&2
  exit 1
fi

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"

node - "$version" <<'NODE'
const fs = require("node:fs");

const version = process.argv[2];

function updateJson(path, update) {
  const value = JSON.parse(fs.readFileSync(path, "utf8"));
  update(value);
  fs.writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

updateJson("package.json", (value) => {
  value.version = version;
});

updateJson("package-lock.json", (value) => {
  value.version = version;
  if (value.packages?.[""]) value.packages[""].version = version;
});

updateJson("src-tauri/tauri.conf.json", (value) => {
  value.version = version;
});

const cargoPath = "src-tauri/Cargo.toml";
const cargo = fs.readFileSync(cargoPath, "utf8");
const packageVersion = /(\[package\][\s\S]*?\nversion = ")[^"]+("\n)/;
if (!packageVersion.test(cargo)) {
  throw new Error(`Não foi possível localizar a versão em ${cargoPath}.`);
}
fs.writeFileSync(
  cargoPath,
  cargo.replace(packageVersion, (_match, prefix, suffix) => `${prefix}${version}${suffix}`),
);

const lockPath = "Cargo.lock";
const lock = fs.readFileSync(lockPath, "utf8");
const appVersion = /(\[\[package\]\]\nname = "olhorganizer"\nversion = ")[^"]+("\n)/;
if (!appVersion.test(lock)) {
  throw new Error(`Não foi possível localizar o pacote olhorganizer em ${lockPath}.`);
}
fs.writeFileSync(
  lockPath,
  lock.replace(appVersion, (_match, prefix, suffix) => `${prefix}${version}${suffix}`),
);
NODE

echo "Versão atualizada para $version."
