#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$HOME/store" "$HOME/Applications"
rm -rf "$HOME/Applications/TextEdit.app"
if [ ! -d /tmp/rs-apps/TextEdit.app ]; then
  cd "$HOME/slopos-i"
  OUTDIR=/tmp/rs-apps bash packaging/apps/build-all-bundles.sh
fi
tar -C /tmp/rs-apps -czf "$HOME/store/TextEdit.app.tar.gz" TextEdit.app
SHA=$(sha256sum "$HOME/store/TextEdit.app.tar.gz" | cut -d' ' -f1)
cat > "$HOME/Applications/catalog.json" <<EOF
[{"name":"TextEdit","bundle_id":"com.slopos.textedit","version":"0.1.0","url":"$HOME/store/TextEdit.app.tar.gz","sha256":"$SHA","size":0}]
EOF
python3 - <<'PY'
import hashlib, json, shutil, tarfile
from pathlib import Path
home = Path.home()
archive = home / "store" / "TextEdit.app.tar.gz"
install = home / "Applications"
sha = hashlib.sha256(archive.read_bytes()).hexdigest()
cat = json.loads((install / "catalog.json").read_text())
assert sha == cat[0]["sha256"], (sha, cat[0]["sha256"])
staging = install / ".staging-smoke"
if staging.exists():
    shutil.rmtree(staging)
staging.mkdir()
with tarfile.open(archive, "r:gz") as tf:
    tf.extractall(staging)
apps = [p for p in staging.iterdir() if p.is_dir() and p.name.endswith(".app")]
assert len(apps) == 1, apps
dest = install / apps[0].name
if dest.exists():
    shutil.rmtree(dest)
apps[0].rename(dest)
shutil.rmtree(staging)
(install / ".slopos-rescan").write_text("1\n")
assert (dest / "bin" / "textedit").exists()
print("INSTALLED-VIA-STORE")
print(dest)
PY
test -x "$HOME/Applications/TextEdit.app/bin/textedit"
echo DONE