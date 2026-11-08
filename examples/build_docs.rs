//! Generador de documentación navegable (single-file HTML, sin Node ni red).
//!
//! Lee `docs/**/*.<lang>.md` y produce `docs/site/<lang>/index.html`
//! con sidebar, índice de la página (TOC), routing por hash y búsqueda.
//! La fuente de verdad sigue siendo Markdown; el HTML es generado (nunca se
//! edita a mano).
//!
//! Uso: `cargo run --example build_docs -- --lang es`
//! PowerShell: `.\scripts\build_docs.ps1 -Lang es`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Proyecto publicado, usado en la cabecera y el pie del sitio.
const REPO: &str = "https://github.com/ionixcorp/cripsy_binary_API";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct Pagina {
    id: String,
    titulo: String,
    grupo: String,
    html: String,
    toc: String,
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convierte Markdown (subconjunto) a HTML.
fn md_a_html(md: &str) -> String {
    let mut out = String::new();
    let mut en_codigo = false;
    let mut codigo_lang = String::new();
    let mut codigo_buf = String::new();
    let mut lista_abierta: Option<&str> = None; // "ul" | "ol"
    let mut tabla_buf: Vec<Vec<String>> = Vec::new();

    macro_rules! cerrar_lista {
        () => {
            if let Some(t) = lista_abierta.take() {
                out.push_str(&format!("</{t}>"));
            }
        };
    }
    macro_rules! cerrar_tabla {
        () => {
            if !tabla_buf.is_empty() {
                let filas = std::mem::take(&mut tabla_buf);
                if !filas.is_empty() {
                    out.push_str("<table>");
                    for (i, fila) in filas.iter().enumerate() {
                        let etiqueta = if i == 0 { "th" } else { "td" };
                        out.push_str("<tr>");
                        for celda in fila {
                            out.push_str(&format!("<{etiqueta}>{}</{etiqueta}>", inline(celda)));
                        }
                        out.push_str("</tr>");
                    }
                    out.push_str("</table>");
                }
            }
        };
    }

    for linea in md.lines() {
        let l = linea.trim_end();
        if l.trim_start().starts_with("```") {
            if en_codigo {
                let clase = if codigo_lang.is_empty() {
                    "txt".to_string()
                } else {
                    codigo_lang.clone()
                };
                out.push_str(&format!(
                    "<pre class=\"code\"><code class=\"lang-{clase}\">{}</code></pre>",
                    esc(&codigo_buf),
                    clase = clase
                ));
                en_codigo = false;
                codigo_buf.clear();
            } else {
                cerrar_lista!();
                cerrar_tabla!();
                en_codigo = true;
                codigo_lang = l.trim_start().trim_start_matches("```").trim().to_string();
            }
            continue;
        }
        if en_codigo {
            codigo_buf.push_str(l);
            codigo_buf.push('\n');
            continue;
        }
        // Líneas de tabla
        if l.trim_start().starts_with('|') && l.trim_start().ends_with('|') {
            cerrar_lista!();
            let celdas: Vec<String> = l
                .trim()
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect();
            // Separador |---|---|
            if celdas
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
            {
                continue;
            }
            tabla_buf.push(celdas);
            continue;
        } else {
            cerrar_tabla!();
        }

        let t = l.trim();
        if t.is_empty() {
            cerrar_lista!();
            continue;
        }
        // Encabezados
        if let Some(rest) = t.strip_prefix("###### ") {
            cerrar_lista!();
            out.push_str(&format!("<h6>{}</h6>", inline(rest)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("##### ") {
            cerrar_lista!();
            out.push_str(&format!("<h5>{}</h5>", inline(rest)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("#### ") {
            cerrar_lista!();
            out.push_str(&format!("<h4>{}</h4>", inline(rest)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("### ") {
            cerrar_lista!();
            out.push_str(&format!("<h3>{}</h3>", inline(rest)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("## ") {
            cerrar_lista!();
            out.push_str(&format!("<h2>{}</h2>", inline(rest)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("# ") {
            cerrar_lista!();
            out.push_str(&format!("<h1>{}</h1>", inline(rest)));
            continue;
        }
        // Lista
        if let Some(rest) = t.strip_prefix("- ") {
            if lista_abierta != Some("ul") {
                cerrar_lista!();
                out.push_str("<ul>");
                lista_abierta = Some("ul");
            }
            out.push_str(&format!("<li>{}</li>", inline(rest)));
            continue;
        }
        if t.len() > 2 && t.as_bytes()[0].is_ascii_digit() {
            if let Some(pos) = t.find(". ") {
                let resto = &t[pos + 2..];
                if lista_abierta != Some("ol") {
                    cerrar_lista!();
                    out.push_str("<ol>");
                    lista_abierta = Some("ol");
                }
                out.push_str(&format!("<li>{}</li>", inline(resto)));
                continue;
            }
        }
        // Cita
        if let Some(rest) = t.strip_prefix("> ") {
            cerrar_lista!();
            out.push_str(&format!("<blockquote>{}</blockquote>", inline(rest)));
            continue;
        }
        // Reglahorizontal
        if t == "---" || t == "***" {
            cerrar_lista!();
            out.push_str("<hr>");
            continue;
        }
        // Párrafo: acumula líneas contiguas
        cerrar_lista!();
        out.push_str(&format!("<p>{}</p>", inline(t)));
    }
    cerrar_lista!();
    cerrar_tabla!();
    out
}

/// Formato inline: **negrita**, `codigo`, [texto](url). Seguro para UTF-8
/// (recorrido por caracteres, sin cortes por byte).
fn inline(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut negrita = false;
    while i < chars.len() {
        // ** negrita **
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            out.push_str(if negrita { "</strong>" } else { "<strong>" });
            negrita = !negrita;
            i += 2;
            continue;
        }
        // `codigo`
        if chars[i] == '`' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            if j < chars.len() {
                let contenido: String = chars[i + 1..j].iter().collect();
                out.push_str(&format!("<code>{}</code>", esc(&contenido)));
                i = j + 1;
                continue;
            }
        }
        // [texto](url)
        if chars[i] == '[' {
            let mut j = i + 1;
            while j + 1 < chars.len() && !(chars[j] == ']' && chars[j + 1] == '(') {
                j += 1;
            }
            if j + 1 < chars.len() && chars[j] == ']' && chars[j + 1] == '(' {
                let mut k = j + 2;
                while k < chars.len() && chars[k] != ')' {
                    k += 1;
                }
                if k < chars.len() {
                    let texto: String = chars[i + 1..j].iter().collect();
                    let url: String = chars[j + 2..k].iter().collect();
                    out.push_str(&format!("<a href=\"{}\">{}</a>", esc(&url), esc(&texto)));
                    i = k + 1;
                    continue;
                }
            }
        }
        match chars[i] {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            otro => out.push(otro),
        }
        i += 1;
    }
    if negrita {
        out.push_str("</strong>");
    }
    out
}

fn slug(path: &Path, lang: &str) -> String {
    let rel = path.to_string_lossy().replace('\\', "/");
    rel.trim_end_matches(&format!(".{lang}.md"))
        .replace("docs/", "")
        .replace(['/', ' '], "-")
        .to_lowercase()
}

/// Ancla estable a partir del texto visible de un encabezado.
fn slug_texto(t: &str) -> String {
    let mut s = String::new();
    let mut guion = false;
    for c in t.chars() {
        let c = c.to_lowercase().next().unwrap_or(c);
        if c.is_alphanumeric() {
            s.push(c);
            guion = false;
        } else if !guion && !s.is_empty() {
            s.push('-');
            guion = true;
        }
    }
    s.trim_matches('-').to_string()
}

/// Quita etiquetas y entidades para usar el texto como ancla.
fn sin_etiquetas(t: &str) -> String {
    let mut s = String::new();
    let mut dentro = false;
    for c in t.chars() {
        match c {
            '<' => dentro = true,
            '>' => dentro = false,
            c if !dentro => s.push(c),
            _ => {}
        }
    }
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

/// Añade `id` a los `h2`/`h3` y devuelve el HTML con anclas y el índice de la
/// página (TOC) que se muestra en la columna derecha.
fn ids_y_toc(html: &str) -> (String, String) {
    let mut salida = String::with_capacity(html.len() + 512);
    let mut toc = String::new();
    let mut resto = html;
    let mut n = 0usize;
    loop {
        let encontrado = [("h2", "<h2>"), ("h3", "<h3>")]
            .iter()
            .filter_map(|(tag, marca)| resto.find(marca).map(|p| (p, *tag, *marca)))
            .min_by_key(|(p, _, _)| *p);
        let Some((pos, tag, marca)) = encontrado else {
            salida.push_str(resto);
            break;
        };
        let cierre = format!("</{tag}>");
        let inicio = pos + marca.len();
        let Some(fin_rel) = resto[inicio..].find(&cierre) else {
            salida.push_str(resto);
            break;
        };
        let fin = inicio + fin_rel;
        let texto = &resto[inicio..fin];
        n += 1;
        let id = format!("s{n}-{}", slug_texto(&sin_etiquetas(texto)));
        salida.push_str(&resto[..pos]);
        salida.push_str(&format!("<{tag} id=\"{id}\">{texto}</{tag}>"));
        let clase = format!("toc-{}", tag.trim_start_matches('h'));
        toc.push_str(&format!("<a class=\"{clase}\" href=\"#{id}\">{texto}</a>"));
        resto = &resto[fin + cierre.len()..];
    }
    (salida, toc)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut lang = "es".to_string();
    let mut root = ".".to_string();
    let mut out_dir = "docs/site".to_string();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--lang" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    lang = v.clone();
                }
            }
            "--out" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    out_dir = v.clone();
                }
            }
            _ => {
                root = args[i].clone();
            }
        }
        i += 1;
    }

    let base = Path::new(&root).join("docs");
    let sufijo = format!(".{lang}.md");
    let mut archivos: Vec<PathBuf> = Vec::new();
    recopilar(&base, &sufijo, &mut archivos);
    archivos.sort();

    let mut paginas: Vec<Pagina> = Vec::new();
    for ruta in &archivos {
        let md = match fs::read_to_string(ruta) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("no se pudo leer {}: {e}", ruta.display());
                continue;
            }
        };
        let rel = ruta
            .strip_prefix(&base)
            .unwrap_or(ruta)
            .to_string_lossy()
            .replace('\\', "/");
        let grupo = rel.split('/').next().unwrap_or("general").to_string();
        let nombre = rel
            .rsplit('/')
            .next()
            .unwrap_or(&rel)
            .trim_end_matches(&sufijo)
            .to_string();
        let id = slug(ruta, &lang);
        // Título: primer encabezado `# `, o el nombre del archivo
        let titulo = md
            .lines()
            .find_map(|l| l.strip_prefix("# ").map(str::to_string))
            .unwrap_or_else(|| {
                // Quita prefijos numéricos tipo "01_" y los guiones bajos.
                nombre
                    .split_once('_')
                    .map(|(_, resto)| resto)
                    .unwrap_or(&nombre)
                    .replace('_', " ")
            });
        let (html, toc) = ids_y_toc(&md_a_html(&md));
        paginas.push(Pagina {
            id,
            titulo,
            grupo,
            html,
            toc,
        });
    }

    let nav_grupos: BTreeMap<String, Vec<(String, String, String)>> = {
        let mut m: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
        for p in &paginas {
            m.entry(p.grupo.clone()).or_default().push((
                p.id.clone(),
                p.grupo.clone(),
                p.titulo.clone(),
            ));
        }
        m
    };

    let mut nav_html = String::new();
    for (grupo, items) in &nav_grupos {
        nav_html.push_str(&format!(
            "<div class=\"nav-grupo\"><div class=\"nav-titulo\">{}</div>",
            esc(&nombre_grupo(grupo))
        ));
        for (id, _g, titulo) in items {
            nav_html.push_str(&format!(
                "<a class=\"nav-item\" href=\"#{id}\" data-id=\"{id}\">{}</a>",
                esc(titulo)
            ));
        }
        nav_html.push_str("</div>");
    }

    let mut contenido_html = String::new();
    for p in &paginas {
        contenido_html.push_str(&format!(
            "<section class=\"pagina\" id=\"{}\"><h1 class=\"titulo-pagina\">{}</h1>{}</section>",
            p.id,
            esc(&p.titulo),
            p.html
        ));
    }

    let busqueda_json: String = paginas
        .iter()
        .map(|p| {
            format!(
                "{{\"id\":\"{}\",\"t\":{},\"g\":{}}}",
                p.id,
                serde_json_string(&p.titulo),
                serde_json_string(&p.grupo)
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let tocs_json: String = paginas
        .iter()
        .map(|p| format!("\"{}\":{}", p.id, serde_json_string(&p.toc)))
        .collect::<Vec<_>>()
        .join(",");

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>cripsy_binary_API — Documentación</title>
<meta name="description" content="Documentación de cripsy_binary_API: cliente Rust para la API de IQ Option.">
<style>
:root {{ --bg:#0b0e14; --panel:#11151f; --panel2:#151b28; --borde:#1f2736; --texto:#dde3ee; --suave:#8d97a8; --acento:#6aa8ff; --codigo:#0a0d13; }}
*, *::before, *::after {{ box-sizing:border-box; }}
html {{ scroll-behavior:smooth; }}
body {{ margin:0; background:var(--bg); color:var(--texto); font-family:"Segoe UI",system-ui,-apple-system,"Helvetica Neue",sans-serif; font-size:15px; line-height:1.6; display:grid; grid-template-columns:296px minmax(0,1fr) 264px; grid-template-rows:56px 1fr auto; min-height:100vh; }}
header.topbar {{ grid-column:1 / -1; display:flex; align-items:center; gap:14px; padding:0 22px; background:linear-gradient(180deg,#121826,#0e131d); border-bottom:1px solid var(--borde); position:sticky; top:0; z-index:20; }}
.marca {{ display:flex; align-items:center; gap:9px; font-weight:600; letter-spacing:.2px; color:var(--texto); text-decoration:none; }}
.marca .punto {{ width:9px; height:9px; border-radius:50%; background:var(--acento); box-shadow:0 0 12px var(--acento); }}
.marca .ver {{ font-size:11px; font-weight:500; color:var(--suave); border:1px solid var(--borde); border-radius:999px; padding:1px 8px; }}
header.topbar .sep {{ flex:1; }}
header.topbar a.enlace {{ font-size:13px; color:var(--suave); text-decoration:none; padding:6px 10px; border-radius:8px; }}
header.topbar a.enlace:hover {{ color:var(--texto); background:#1a2130; }}
#menu-movil {{ display:none; }}
nav {{ grid-column:1; grid-row:2 / span 2; background:var(--panel); border-right:1px solid var(--borde); padding:18px 14px 28px; position:sticky; top:56px; align-self:start; height:calc(100vh - 56px); overflow-y:auto; }}
nav .rotulo {{ font-size:11px; text-transform:uppercase; letter-spacing:.1em; color:var(--suave); margin:0 0 10px 4px; }}
#buscador {{ width:100%; padding:9px 11px; border-radius:9px; border:1px solid var(--borde); background:var(--codigo); color:var(--texto); margin-bottom:14px; }}
#buscador:focus {{ outline:none; border-color:var(--acento); box-shadow:0 0 0 3px rgba(106,168,255,.15); }}
.nav-grupo {{ margin-bottom:16px; }}
.nav-titulo {{ font-size:11px; text-transform:uppercase; letter-spacing:.08em; color:var(--suave); margin:8px 0 6px 4px; }}
.nav-item {{ display:block; padding:7px 10px; border-radius:8px; color:#c3cbd9; text-decoration:none; font-size:13.5px; border-left:2px solid transparent; }}
.nav-item:hover {{ background:var(--panel2); color:var(--texto); }}
.nav-item.activo {{ background:#1b2740; color:var(--acento); border-left-color:var(--acento); }}
main {{ grid-column:2; padding:34px 46px 64px; max-width:900px; }}
.pagina {{ display:none; }}
.pagina.activa {{ display:block; animation:aparece .18s ease-out; }}
@keyframes aparece {{ from {{ opacity:0; transform:translateY(4px); }} to {{ opacity:1; transform:none; }} }}
h1.titulo-pagina {{ font-size:28px; line-height:1.25; margin:0 0 22px; padding-bottom:14px; border-bottom:1px solid var(--borde); }}
h2 {{ font-size:20px; margin:32px 0 12px; scroll-margin-top:72px; }}
h3 {{ font-size:16.5px; margin:24px 0 10px; color:#e8edf7; scroll-margin-top:72px; }}
p, li {{ font-size:14.5px; color:#cfd6e4; }}
blockquote {{ border-left:3px solid var(--acento); background:#0f1622; margin:16px 0; padding:10px 16px; border-radius:0 8px 8px 0; color:#b9c2d2; }}
pre.code {{ background:var(--codigo); border:1px solid var(--borde); border-left:3px solid #2f57a0; border-radius:10px; padding:14px 16px; overflow-x:auto; font-size:13px; line-height:1.55; }}
code {{ font-family:"Cascadia Code",Consolas,"SF Mono",monospace; background:#161d2b; border:1px solid #222c3d; border-radius:5px; padding:1px 5px; font-size:12.8px; }}
pre code {{ background:none; border:none; padding:0; }}
table {{ border-collapse:separate; border-spacing:0; margin:16px 0; width:100%; font-size:13.5px; border:1px solid var(--borde); border-radius:10px; overflow:hidden; }}
th, td {{ padding:9px 12px; text-align:left; border-bottom:1px solid var(--borde); vertical-align:top; }}
th {{ background:#161d2b; color:#e8edf7; font-weight:600; }}
tr:last-child td {{ border-bottom:none; }}
table tr:nth-child(even) td {{ background:#0e141f; }}
a {{ color:var(--acento); text-decoration:none; }}
a:hover {{ text-decoration:underline; }}
hr {{ border:none; border-top:1px solid var(--borde); margin:26px 0; }}
aside.toc {{ grid-column:3; grid-row:2; position:sticky; top:56px; align-self:start; height:calc(100vh - 56px); overflow-y:auto; padding:26px 18px 40px; border-left:1px solid var(--borde); background:rgba(17,21,31,.55); }}
.toc-titulo {{ font-size:11px; text-transform:uppercase; letter-spacing:.1em; color:var(--suave); margin:0 0 10px; }}
#toc a {{ display:block; padding:5px 0 5px 12px; color:#a9b3c4; font-size:13px; border-left:2px solid var(--borde); }}
#toc a:hover {{ color:var(--texto); border-left-color:var(--acento); text-decoration:none; }}
#toc a.toc-3 {{ padding-left:24px; font-size:12.5px; color:#8d97a8; }}
.toc-vacio {{ font-size:13px; color:var(--suave); }}
.toc-bloque {{ margin-top:26px; padding-top:18px; border-top:1px solid var(--borde); }}
.toc-bloque a {{ display:block; font-size:13px; color:#a9b3c4; padding:4px 0; }}
footer {{ grid-column:1 / -1; display:flex; gap:18px; flex-wrap:wrap; align-items:center; padding:18px 22px; border-top:1px solid var(--borde); background:var(--panel); color:var(--suave); font-size:12.5px; }}
footer a {{ color:var(--suave); }}
footer code {{ font-size:12px; }}
@media (max-width:1120px) {{ body {{ grid-template-columns:1fr; }} nav, aside.toc {{ display:none; }} main {{ grid-column:1; padding:24px 20px 48px; }} header.topbar {{ padding:0 14px; }} #menu-movil {{ display:block; flex:1; max-width:320px; padding:7px 10px; border-radius:8px; border:1px solid var(--borde); background:var(--codigo); color:var(--texto); }} header.topbar a.enlace {{ display:none; }} }}
@media print {{ nav, aside.toc, header.topbar, footer {{ display:none; }} main {{ max-width:none; }} }}
</style>
</head>
<body>
<header class="topbar">
  <a class="marca" href="#{primero}"><span class="punto"></span>cripsy_binary_API <span class="ver">v{version}</span></a>
  <select id="menu-movil" aria-label="Ir a una pagina"></select>
  <span class="sep"></span>
  <a class="enlace" href="{repo}">Repositorio</a>
  <a class="enlace" href="{repo}/tree/master/docs">Markdown</a>
</header>
<nav>
  <div class="rotulo">Documentación</div>
  <input id="buscador" type="search" placeholder="Buscar…" aria-label="Buscar en la documentacion">
  <div id="nav">{nav_html}</div>
</nav>
<main id="contenido">{contenido_html}</main>
<aside class="toc">
  <div class="toc-titulo">En esta página</div>
  <div id="toc"></div>
  <div class="toc-bloque">
    <div class="toc-titulo">Enlaces</div>
    <a href="{repo}">Repositorio en GitHub ↗</a>
    <a href="{repo}/tree/master/installation">Instalación</a>
    <a href="{repo}/tree/master/scripts">Scripts de verificación</a>
  </div>
</aside>
<footer>
  <span>cripsy_binary_API · v{version} · MIT</span>
  <a href="{repo}">github.com/ionixcorp/cripsy_binary_API</a>
  <span>Generado desde Markdown en <code>docs/</code></span>
</footer>
<script>
var DOCS = [{busqueda_json}];
var TOCS = {{{tocs_json}}};
function mostrar(id) {{
  var seccion = document.getElementById(id);
  if (!seccion) {{ return; }}
  document.querySelectorAll('.pagina').forEach(function(s) {{ s.classList.remove('activa'); }});
  document.querySelectorAll('.nav-item').forEach(function(a) {{ a.classList.remove('activo'); }});
  seccion.classList.add('activa');
  var enlace = document.querySelector('.nav-item[data-id="' + id + '"]');
  if (enlace) {{ enlace.classList.add('activo'); enlace.scrollIntoView({{ block:'nearest' }}); }}
  var caja = document.getElementById('toc');
  caja.innerHTML = TOCS[id] || '<div class="toc-vacio">Esta página no tiene secciones.</div>';
  var selector = document.getElementById('menu-movil');
  if (selector) {{ selector.value = id; }}
  window.scrollTo(0, 0);
}}
document.querySelectorAll('.nav-item').forEach(function(a) {{
  a.addEventListener('click', function(e) {{
    e.preventDefault();
    var id = a.getAttribute('data-id');
    location.hash = id;
    mostrar(id);
  }});
}});
document.getElementById('buscador').addEventListener('input', function(e) {{
  var q = e.target.value.toLowerCase().trim();
  document.querySelectorAll('.nav-item').forEach(function(a) {{
    var id = a.getAttribute('data-id');
    var doc = DOCS.find(function(d) {{ return d.id === id; }});
    var visible = !q || (doc && (doc.t.toLowerCase().includes(q) || doc.g.toLowerCase().includes(q)));
    a.style.display = visible ? 'block' : 'none';
  }});
}});
document.getElementById('toc').addEventListener('click', function(e) {{
  var a = e.target.closest('a');
  if (!a) {{ return; }}
  e.preventDefault();
  var destino = document.getElementById(a.getAttribute('href').slice(1));
  if (destino) {{ destino.scrollIntoView({{ behavior:'smooth', block:'start' }}); }}
}});
(function () {{
  var selector = document.getElementById('menu-movil');
  if (!selector) {{ return; }}
  DOCS.forEach(function(d) {{
    var opcion = document.createElement('option');
    opcion.value = d.id;
    opcion.textContent = d.t;
    selector.appendChild(opcion);
  }});
  selector.addEventListener('change', function () {{
    location.hash = selector.value;
    mostrar(selector.value);
  }});
}})();
window.addEventListener('hashchange', function () {{
  mostrar(location.hash.replace('#','') || '{primero}');
}});
var primero = '{primero}';
if (location.hash) {{ mostrar(location.hash.replace('#','')); }} else {{ mostrar(primero); }}
</script>
</body>
</html>
"##,
        lang = lang,
        version = VERSION,
        repo = REPO,
        nav_html = nav_html,
        contenido_html = contenido_html,
        busqueda_json = busqueda_json,
        tocs_json = tocs_json,
        primero = paginas
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "inicio".into()),
    );

    let destino = Path::new(&out_dir).join(&lang);
    if let Err(e) = fs::create_dir_all(&destino) {
        eprintln!("no se pudo crear {}: {e}", destino.display());
        std::process::exit(1);
    }
    let ruta_html = destino.join("index.html");
    if let Err(e) = fs::write(&ruta_html, &html) {
        eprintln!("no se pudo escribir {}: {e}", ruta_html.display());
        std::process::exit(1);
    }
    println!(
        "documentación generada: {} ({} páginas)",
        ruta_html.display(),
        paginas.len()
    );
}

fn recopilar(dir: &Path, sufijo: &str, salida: &mut Vec<PathBuf>) {
    if let Ok(entradas) = fs::read_dir(dir) {
        for e in entradas.flatten() {
            let ruta = e.path();
            if ruta.is_dir() {
                recopilar(&ruta, sufijo, salida);
            } else if let Some(nombre) = ruta.file_name().and_then(|n| n.to_str()) {
                if nombre.ends_with(sufijo) {
                    salida.push(ruta);
                }
            }
        }
    }
}

fn nombre_grupo(g: &str) -> String {
    let sin_num = g.split_once('_').map(|(_, b)| b).unwrap_or(g);
    let titulo: String = sin_num
        .split('_')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    titulo
}

fn serde_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
