"""Render the wiki pages to static HTML (GitHub Pages serves them as-is)."""

import pathlib
import markdown

ROOT = pathlib.Path(__file__).parent
PAGES = ["index", "Install", "Effect", "Updating", "Architecture", "Troubleshooting", "Releases"]
TITLES = {
    "index": "XM6 360",
    "Install": "التركيب",
    "Effect": "استخدام التأثير",
    "Updating": "التحديث من upstream",
    "Architecture": "البنية",
    "Troubleshooting": "استكشاف الأخطاء",
    "Releases": "الإصدارات",
}

TEMPLATE = """<!doctype html>
<html lang="ar" dir="rtl">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — XM6 360</title>
<style>
:root {{ --fg:#1c1e21; --muted:#5b6167; --line:#dfe3e8; --bg:#fbfbfc; --card:#fff; --accent:#0a6cff; }}
@media (prefers-color-scheme: dark) {{
  :root {{ --fg:#e6e8eb; --muted:#a0a6ad; --line:#2c3036; --bg:#14161a; --card:#1b1e23; --accent:#6ba6ff; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--bg); color:var(--fg);
  font: 16px/1.75 "Segoe UI", Tahoma, "Noto Naskh Arabic", system-ui, sans-serif; }}
header {{ border-bottom:1px solid var(--line); background:var(--card); }}
nav {{ max-width:900px; margin:0 auto; padding:14px 20px; display:flex; flex-wrap:wrap; gap:6px 16px; align-items:baseline; }}
nav .brand {{ font-weight:700; margin-inline-end:auto; }}
nav a {{ color:var(--muted); text-decoration:none; font-size:14px; }}
nav a:hover, nav a[aria-current] {{ color:var(--accent); }}
main {{ max-width:900px; margin:0 auto; padding:28px 20px 64px; }}
h1 {{ font-size:30px; line-height:1.3; margin:0 0 18px; }}
h2 {{ font-size:21px; margin:34px 0 10px; padding-top:14px; border-top:1px solid var(--line); }}
h3 {{ font-size:17px; margin:22px 0 8px; }}
p, li {{ color:var(--fg); }}
a {{ color:var(--accent); }}
code {{ background:rgba(127,127,127,.14); padding:1px 5px; border-radius:4px; font-size:14px; direction:ltr; unicode-bidi:embed; }}
pre {{ background:var(--card); border:1px solid var(--line); border-radius:10px; padding:14px 16px; overflow:auto; direction:ltr; text-align:left; }}
pre code {{ background:none; padding:0; }}
table {{ width:100%; border-collapse:collapse; margin:14px 0; font-size:15px; }}
th, td {{ border:1px solid var(--line); padding:8px 10px; text-align:right; vertical-align:top; }}
th {{ background:rgba(127,127,127,.08); }}
blockquote {{ margin:16px 0; padding:2px 16px; border-inline-start:3px solid var(--accent); color:var(--muted); }}
hr {{ border:0; border-top:1px solid var(--line); margin:28px 0; }}
footer {{ max-width:900px; margin:0 auto; padding:0 20px 40px; color:var(--muted); font-size:13px; }}
</style>
</head>
<body>
<header><nav>
  <span class="brand">XM6 360</span>
  {nav}
</nav></header>
<main>
{body}
</main>
<footer>A2DP Windows Bridge — فرع <code>Akramx15/A2DP-Windows-Bridge</code></footer>
</body>
</html>
"""


def nav(current: str) -> str:
    return "\n  ".join(
        f'<a href="{p}.html"{" aria-current=\"page\"" if p == current else ""}>{TITLES[p]}</a>'
        for p in PAGES
    )


for name in PAGES:
    text = (ROOT / f"{name}.md").read_text(encoding="utf-8")
    body = markdown.markdown(text, extensions=["tables", "fenced_code", "sane_lists"])
    (ROOT / f"{name}.html").write_text(
        TEMPLATE.format(title=TITLES[name], nav=nav(name), body=body), encoding="utf-8"
    )
    print(f"{name}.html")
