#!/usr/bin/env bash
# Exercise the installed executable from outside a checkout, before npm setup.
set -euo pipefail
atlas_source=$(cd "$(dirname "$0")/.." && pwd)
atlas_scratch=$(mktemp -d "${TMPDIR:-/tmp}/atlas-install.XXXXXX")
trap 'rm -rf "$atlas_scratch"' EXIT
cargo install --locked --path "$atlas_source" --root "$atlas_scratch/install"
mkdir -p "$atlas_scratch/project/src" "$atlas_scratch/work"
cat > "$atlas_scratch/project/Cargo.toml" <<'MANIFEST'
[package]
name = "installed-atlas-fixture"
version = "0.0.0"
edition = "2024"
MANIFEST
cat > "$atlas_scratch/project/src/lib.rs" <<'SOURCE'
pub mod transformer;
pub use transformer::{Input, Output, transform};
enum Layer { Stage }
const MODULES: [(&str, Option<()>, Layer, &[&str]);1] = [("transformer",None,Layer::Stage,&[])];
const SURFACES: [(&str,&str,&[&str]);1] = [("transformer","transform",&["Input","Output"])];
SOURCE
cat > "$atlas_scratch/project/src/transformer.rs" <<'SOURCE'
pub struct Input;
pub struct Output;
pub fn transform(_: Input) -> Output { Output }
SOURCE
cat > "$atlas_scratch/project/src/main.rs" <<'SOURCE'
fn main() { let _ = installed_atlas_fixture::transform(installed_atlas_fixture::Input); }
SOURCE
cat > "$atlas_scratch/project/profile.json" <<'PROFILE'
{"schema":1,"source":[],"modules":[{"name":"transformer","kind":"stage","path":"src/lib.rs","entry":"transform","types":["Input","Output"],"dependencies":[]}],"workflows":[]}
PROFILE
test -x "$atlas_scratch/install/bin/atlas-girard"
cd "$atlas_scratch/work"
"$atlas_scratch/install/bin/atlas" --version
"$atlas_scratch/install/bin/atlas" census "$atlas_scratch/project" --json > census.json
"$atlas_scratch/install/bin/atlas" map "$atlas_scratch/project" --level files --backend ascii --out "$atlas_scratch/work/map.txt"
"$atlas_scratch/install/bin/atlas" map "$atlas_scratch/project" --level files --backend pele --format svg --out "$atlas_scratch/work/map.svg"
"$atlas_scratch/install/bin/atlas" program "$atlas_scratch/project" --architecture profile.json --out "$atlas_scratch/work/program.txt"
"$atlas_scratch/install/bin/atlas" program "$atlas_scratch/project" --architecture profile.json --backend pele --out "$atlas_scratch/work/program.html"
node --input-type=module -e 'import fs from "node:fs"; for (const path of ["program.txt", "program.html"]) if (!fs.readFileSync(path, "utf8").includes("transformer")) process.exit(1);'
test -s census.json
"$atlas_scratch/install/bin/atlas" --root "$atlas_scratch/project" census --json > census-root.json
"$atlas_scratch/install/bin/atlas" name Output --root "$atlas_scratch/project" --json > name-root.json
node --input-type=module <<'JS'
import fs from 'node:fs';
if (fs.readFileSync('census.json', 'utf8') !== fs.readFileSync('census-root.json', 'utf8'))
    throw new Error('canonical root changed the installed source population');
const name = JSON.parse(fs.readFileSync('name-root.json', 'utf8'));
if (!JSON.stringify(name).includes('Output')) throw new Error('canonical name consumed its selector as a root');
JS
test -s map.txt
node --input-type=module -e 'import fs from "node:fs"; const svg = fs.readFileSync("map.svg", "utf8"); if (!svg.includes("<svg") || !svg.includes("lib.rs")) process.exit(1);'

cat > "$atlas_scratch/project/representations.json" <<'QUERY'
{"title":"Installed representation query","types":[{"name":"Input"},{"name":"Output"}],"preparation_files":["src/lib.rs"]}
QUERY
"$atlas_scratch/install/bin/atlas" representations "$atlas_scratch/project" --query representations.json > representations.txt
"$atlas_scratch/install/bin/atlas" representations "$atlas_scratch/project" --query representations.json --backend pele --out "$atlas_scratch/work/representations.svg"
node --input-type=module -e 'import fs from "node:fs"; for (const path of ["representations.txt", "representations.svg"]) if (!fs.readFileSync(path, "utf8").includes("transform")) process.exit(1);'

"$atlas_scratch/install/bin/atlas" overview "$atlas_scratch/project" --backend pele --out "$atlas_scratch/work/overview.svg"
"$atlas_scratch/install/bin/atlas" dependencies "$atlas_scratch/project" src --relation all --backend pele --out "$atlas_scratch/work/dependencies.svg"
"$atlas_scratch/install/bin/atlas" representations "$atlas_scratch/project" --type Output --panel producers > producers.txt
git -C "$atlas_scratch/project" init -q
git -C "$atlas_scratch/project" add Cargo.toml src
git -C "$atlas_scratch/project" -c user.name='Atlas installation test' -c user.email=atlas@example.invalid commit -qm baseline
"$atlas_scratch/install/bin/atlas" compare "$atlas_scratch/project" --before HEAD --backend pele --out "$atlas_scratch/work/comparison.svg"
node --input-type=module -e 'import fs from "node:fs"; for (const path of ["overview.svg", "dependencies.svg", "comparison.svg"]) if (!fs.readFileSync(path, "utf8").includes("<svg")) process.exit(1);'

"$atlas_scratch/install/bin/atlas" name "$atlas_scratch/project" Output --relations > names.txt
"$atlas_scratch/install/bin/atlas" flow "$atlas_scratch/project" --path src/main.rs --function main --depth 1 --backend pele --format svg --out "$atlas_scratch/work/flow.svg"
node --input-type=module -e 'import fs from "node:fs"; const svg=fs.readFileSync("flow.svg","utf8"); if(!svg.includes("return to")||!svg.includes("transformer.rs")) process.exit(1);'

"$atlas_scratch/install/bin/atlas" overview "$atlas_scratch/project" --adapter "$atlas_scratch/install/bin/atlas-girard" --json > adapter.json
node --input-type=module -e 'import fs from "node:fs"; const result=JSON.parse(fs.readFileSync("adapter.json","utf8")); if(result.adapter.producer.name!=="atlas-girard"||result.adapter.snapshot_id!==result.snapshot.id) process.exit(1);'

# Shared-library source reports also run without their source checkout.
for atlas_question in spans nontest-loc concentration; do
    "$atlas_scratch/install/bin/atlas" "$atlas_question" src --root "$atlas_scratch/project" --json > "$atlas_question.json"
done
"$atlas_scratch/install/bin/atlas" spans --scope src --root "$atlas_scratch/project" --json > spans-scope.json
cmp spans.json spans-scope.json
node --input-type=module <<'JS'
import fs from 'node:fs';
for (const question of ['spans', 'nontest-loc', 'concentration']) {
    const report = JSON.parse(fs.readFileSync(`${question}.json`, 'utf8'));
    if (report.schema !== 1 || report.question !== question || report.report.files.length !== 3)
        throw new Error(`installed ${question} lost its source population`);
    const transformer = report.report.files.find(file => file.file === 'src/transformer.rs');
    if (!transformer?.items.some(item => item.name === 'transform') || !transformer.largest_block)
        throw new Error(`installed ${question} lost source evidence`);
}
JS

"$atlas_scratch/install/bin/atlas" diff HEAD --root "$atlas_scratch/project" --json > shape-diff.json
node --input-type=module <<'JS'
import fs from 'node:fs';
const diff = JSON.parse(fs.readFileSync('shape-diff.json', 'utf8'));
if (diff.question !== 'diff' || !/^[0-9a-f]{40}$/.test(diff.basis.before) || diff.basis.after !== null || diff.report.changes.length !== 0)
    throw new Error('installed shape diff lost its frozen basis or invented a production edit');
JS
printf '\npub struct Added;\n' >> "$atlas_scratch/project/src/transformer.rs"
"$atlas_scratch/install/bin/atlas" diff HEAD --root "$atlas_scratch/project" --scope src/transformer.rs --json > shape-changed.json
node --input-type=module <<'JS'
import fs from 'node:fs';
const diff = JSON.parse(fs.readFileSync('shape-changed.json', 'utf8'));
const [change] = diff.report.changes;
if (diff.report.changes.length !== 1 || change.file !== 'src/transformer.rs' || change.after.top_level_items !== change.before.top_level_items + 1 || change.after.fingerprint === change.before.fingerprint)
    throw new Error('installed shape diff lost its production edit or source witnesses');
JS

# A scoped move matches the entire crate first, then reports the destination.
mkdir -p "$atlas_scratch/project/src/backend"
mv "$atlas_scratch/project/src/transformer.rs" "$atlas_scratch/project/src/backend/transformer.rs"
node --input-type=module - "$atlas_scratch/project" <<'JS'
import fs from 'node:fs';
const root = process.argv[2];
const transformer = `${root}/src/backend/transformer.rs`;
fs.writeFileSync(transformer, fs.readFileSync(transformer, 'utf8').replace('pub fn transform', 'pub(crate) fn transform'));
const lib = `${root}/src/lib.rs`;
fs.writeFileSync(lib, fs.readFileSync(lib, 'utf8').replace('pub mod transformer;', '#[path = "backend/transformer.rs"]\npub mod transformer;'));
JS
for atlas_question in moved visibility; do
    "$atlas_scratch/install/bin/atlas" "$atlas_question" HEAD --root "$atlas_scratch/project" --scope src/backend --json > "$atlas_question.json"
done
node --input-type=module <<'JS'
import fs from 'node:fs';
const moved = JSON.parse(fs.readFileSync('moved.json', 'utf8'));
const visibility = JSON.parse(fs.readFileSync('visibility.json', 'utf8'));
if (moved.question !== 'moved' || moved.report.exact !== 3 || moved.report.relocated !== 3 || moved.report.changed.length || moved.report.only_after[0]?.name !== 'Added' || moved.report.unreadable.length || !moved.report.basis.before.sources['src/transformer.rs'] || !moved.report.basis.after.sources['src/backend/transformer.rs'])
    throw new Error('installed move report lost its match across scope, exact tokens or source basis');
const [narrowed] = visibility.report.narrowed;
if (visibility.question !== 'visibility' || visibility.report.narrowed.length !== 1 || narrowed.name !== 'transform' || narrowed.before.visibility !== 'pub' || narrowed.after.visibility !== 'pub(crate)' || !visibility.report.limits.length || Object.hasOwn(visibility, 'verdict'))
    throw new Error('installed visibility report lost the written scope change or overstated authority');
JS

"$atlas_scratch/install/bin/atlas" comments --audit --pub-only --root "$atlas_scratch/project" --json > comments.json
node --input-type=module <<'JS'
import fs from 'node:fs';
const audit = JSON.parse(fs.readFileSync('comments.json', 'utf8'));
if (audit.question !== 'comments' || audit.report.citation_markers !== null || audit.report.headers !== 'all-files' || Object.keys(audit.report.sources).length !== 3 || !audit.report.findings.some(finding => finding.kind === 'missing-outer-line-doc') || Object.hasOwn(audit, 'verdict'))
    throw new Error('installed documentation audit lost selection, source witnesses or rule applicability');
JS
if "$atlas_scratch/install/bin/atlas" comments --root "$atlas_scratch/project" > unexpected-audit.json 2> required-audit.txt; then
    echo 'comments accepted an invocation without --audit' >&2
    exit 1
fi
grep -q -- '--audit' required-audit.txt
# Review tests above moved the first fixture. Keep check populations independent.
atlas_check_project="$atlas_scratch/check-project"
mkdir -p "$atlas_check_project/src"
cp "$atlas_scratch/project/Cargo.toml" "$atlas_check_project/Cargo.toml"
printf 'pub mod transformer;\n' > "$atlas_check_project/src/lib.rs"
printf 'pub struct Input;\npub struct Output;\n' > "$atlas_check_project/src/transformer.rs"
cat > "$atlas_check_project/architecture.toml" <<'POLICY'
schema = 1
provenance = "authored"
surface = {}
authority = []
stage_order = [{crate = "installed-atlas-fixture", stages = ["transformer"]}]
[population]
core = ["installed-atlas-fixture"]
[size]
core_token_lines = 100
crate = {installed-atlas-fixture = 100}
[crate_graph]
rule = []
POLICY
"$atlas_scratch/install/bin/atlas" stages --root "$atlas_check_project" --policy architecture.toml --json > stages.json
node --input-type=module <<'JS'
import fs from 'node:fs';
const report = JSON.parse(fs.readFileSync('stages.json', 'utf8'));
if (report.question !== 'stages' || report.status !== 'report-only' || report.observations.crates['installed-atlas-fixture'].unjudged.length || report.observations.crates['installed-atlas-fixture'].absent.length || Object.hasOwn(report, 'verdict'))
    throw new Error('installed stage report lost its evidence or claimed a complete verdict');
JS
"$atlas_scratch/install/bin/atlas" seams --root "$atlas_check_project" --policy architecture.toml --json > no-seams.json
cat >> "$atlas_check_project/architecture.toml" <<'POLICY'
[stage_seams.installed-atlas-fixture]
floor = []
POLICY
if "$atlas_scratch/install/bin/atlas" seams --root "$atlas_check_project" --policy architecture.toml --json > missing-boundary.json 2> boundary-required.txt; then
    echo 'installed seams invented a project boundary' >&2
    exit 1
fi
node --input-type=module <<'JS'
import fs from 'node:fs';
const absent = JSON.parse(fs.readFileSync('no-seams.json', 'utf8'));
const missing = JSON.parse(fs.readFileSync('missing-boundary.json', 'utf8'));
if (absent.status !== 'not-applicable' || missing.status !== 'unavailable' || !missing.observations.unavailable.length || Object.hasOwn(missing, 'verdict'))
    throw new Error('installed seam command conflated absent rules with unavailable inputs');
JS
grep -q -- 'native Atlas wrapper' boundary-required.txt
