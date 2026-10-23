#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Build the single-file landing page (index.html).

Embeds every Markdown document (gzip + base64) and the docs/img screenshots
(WebP data URIs) into site/index.template.html. Run from anywhere:

    python3 scripts/build-site.py
"""
import base64, gzip, io, json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
GROUPS = [
    ("start",   ["README", "README.ro", "GETTING_STARTED", "FAQ", "GLOSSARY", "INDEX"]),
    ("use",     ["STUDIO", "CLI_REFERENCE", "RIGGING_GUIDE", "ANIMATION_GUIDE", "PROJECT_FORMAT", "EXPORT_FORMATS", "CI"]),
    ("build",   ["ARCHITECTURE", "API", "C_ABI", "WASM", "DEVELOPMENT", "CONTRIBUTING"]),
    ("trust",   ["LIMITATIONS", "SECURITY", "CLEANROOM", "LICENSING", "THIRD_PARTY", "PATENT_NOTES", "TRADEMARK"]),
    ("project", ["ROADMAP", "FEATURES", "CHANGELOG", "RELEASE_CHECKLIST"]),
]
TITLES = {"README": "README", "README.ro": "README (română)", "ROADMAP": "Roadmap",
          "FEATURES": "Feature matrix", "CHANGELOG": "Changelog", "CLEANROOM": "Clean-room provenance",
          "INDEX": "Documentation index"}
EXTRA_DESC = {"INDEX": ("Where to find the right page: all documents, reading paths by audience, questions and answers", "everyone")}
# THIRD_PARTY_NOTICES.md (1.4 MB of generated licence texts) is linked, not embedded.


def find(name):
    for p in (ROOT / f"{name}.md", ROOT / "docs" / f"{name}.md"):
        if p.exists():
            return p
    sys.exit(f"missing document: {name}")


def descriptions():
    d = {}
    idx = (ROOT / "docs" / "INDEX.md").read_text(encoding="utf-8")
    for m in re.finditer(r"^\| \[(\w+)\.md\]\([^)]*\) \| (.*?) \| (.*?) \|$", idx, re.M):
        d[m.group(1)] = (m.group(2), m.group(3))
    root_tab = idx.split("## Documents at the repository root")[1].split("\n## ")[0]
    for m in re.finditer(r"^\| (.*?) \| (.*?) \|$", root_tab, re.M):
        for f in re.findall(r"\[\.\./([\w.]+?)\.md\]", m.group(1)):
            d.setdefault(f, (m.group(2), ""))
    return d


def main():
    desc = descriptions()
    docs, order = {}, []
    for g, ids in GROUPS:
        for i in ids:
            p = find(i)
            md = p.read_text(encoding="utf-8")
            h1 = re.search(r"^# (.+)$", md, re.M)
            title = TITLES.get(i)
            if not title and h1:
                title = re.sub(r"^ARMATURA ", "", h1.group(1).replace("`", ""))
                title = title[:1].upper() + title[1:]
            d, a = desc.get(i) or EXTRA_DESC.get(i, ("", ""))
            docs[i] = {"t": title or i, "g": g,
                       "p": str(p.relative_to(ROOT)), "d": d, "a": a, "md": md}
            order.append(i)
    payload = base64.b64encode(gzip.compress(json.dumps({"order": order, "docs": docs}, ensure_ascii=False).encode(), 9)).decode()

    from PIL import Image
    imgs = {}
    for p in sorted((ROOT / "docs" / "img").glob("*.png")):
        im = Image.open(p).convert("RGB")
        if im.width > 1400:
            im = im.resize((1400, round(im.height * 1400 / im.width)), Image.LANCZOS)
        b = io.BytesIO()
        im.save(b, "WEBP", quality=80, method=6)
        imgs[p.name] = "data:image/webp;base64," + base64.b64encode(b.getvalue()).decode()

    tpl = (ROOT / "site" / "index.template.html").read_text(encoding="utf-8")
    out = tpl.replace("/*__DOCS__*/", payload).replace("/*__IMGS__*/ {}", json.dumps(imgs))
    (ROOT / "index.html").write_text(out, encoding="utf-8")
    print(f"index.html: {len(out)/1024:.0f} KiB, {len(docs)} documents, {len(imgs)} images")


main()
