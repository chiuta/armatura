#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Build the landing page (index.html and site/data/).

Builds index.html from site/index.template.html and the UI strings in site/i18n/*.json, and writes the
Markdown documents (English and translations) to site/data/docs.<lang>.js, loaded on demand by the page.
The screenshots are read from docs/img/. Run from anywhere:

    python3 scripts/build-site.py
"""
import hashlib, importlib.util, json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
GROUPS = [
    ("start",   ["README", "GETTING_STARTED", "FAQ", "GLOSSARY", "INDEX"]),
    ("use",     ["STUDIO", "CLI_REFERENCE", "RIGGING_GUIDE", "ANIMATION_GUIDE", "PROJECT_FORMAT", "EXPORT_FORMATS", "CI"]),
    ("build",   ["ARCHITECTURE", "API", "C_ABI", "WASM", "DEVELOPMENT", "CONTRIBUTING"]),
    ("trust",   ["LIMITATIONS", "SECURITY", "CLEANROOM", "LICENSING", "THIRD_PARTY", "PATENT_NOTES", "TRADEMARK"]),
    ("project", ["ROADMAP", "FEATURES", "CHANGELOG", "RELEASE_CHECKLIST"]),
]
TITLES = {"README": "README", "ROADMAP": "Roadmap",
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
    spec = importlib.util.spec_from_file_location("i18n_tools", ROOT / "scripts" / "i18n.py")
    i18n = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(i18n)

    # Documents are written as separate script files, loaded on demand by index.html:
    # site/data/docs.en.js (the English documents) and site/data/docs.<lang>.js (translations).
    # Plain scripts, not fetch(), so the page also works when opened from disk.
    data = ROOT / "site" / "data"
    data.mkdir(exist_ok=True)
    for old in data.glob("docs.*.js"):
        old.unlink()
    files = {"en": "window.ARM_DATA=window.ARM_DATA||{};ARM_DATA.en=" + dump({"order": order, "docs": docs}) + ";\n"}
    for lang in i18n.LANGS:
        tr = {i: i18n.tr_path(i, lang).read_text(encoding="utf-8") for i in docs if i18n.tr_path(i, lang).exists()}
        if tr:
            files[lang] = f"window.ARM_DATA=window.ARM_DATA||{{}};ARM_DATA.{lang}=" + dump(tr) + ";\n"
    h = hashlib.sha1()
    for lang, text in sorted(files.items()):
        (data / f"docs.{lang}.js").write_text(text, encoding="utf-8")
        h.update(text.encode())

    ui = {p.stem: json.load(open(p, encoding="utf-8")) for p in sorted((ROOT / "site" / "i18n").glob("*.json"))}
    base = set(ui["en"])
    for lang, d in ui.items():
        if set(d) != base:
            sys.exit(f"site/i18n/{lang}.json: keys differ from en.json: {sorted(base ^ set(d))}")

    tpl = (ROOT / "site" / "index.template.html").read_text(encoding="utf-8")
    out = tpl.replace("/*__I18N__*/ {}", dump(ui)).replace("/*__BUILD__*/", h.hexdigest()[:10])
    (ROOT / "index.html").write_text(out, encoding="utf-8")
    size = sum(len(t.encode()) for t in files.values()) / 1024
    print(f"index.html: {len(out)/1024:.0f} KiB; site/data: {len(files)} files, {size:.0f} KiB; {len(docs)} documents")


def dump(obj):
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


if __name__ == "__main__":
    main()
