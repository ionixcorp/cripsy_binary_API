//! Generador de documentación navegable (single-file HTML, sin Node ni red).
//!
//! Lee `docs/**/*.<lang>.md` y produce `docs/site/<lang>/index.html`
//! con sidebar, routing por hash y búsqueda. La fuente de verdad sigue siendo
//! Markdown; el HTML es generado (nunca se edita a mano).
//!
//! Uso: `cargo run --example build_docs -- --lang es`
//! PowerShell: `.\scripts\build_docs.ps1 -Lang es`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Pagina {
    id: String,
    titulo: String,
    grupo: String,
    html: String,
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
        let html = md_a_html(&md);
        paginas.push(Pagina {
            id,
            titulo,
            grupo,
            html,
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

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>cripsy_binary_API — Documentación</title>
<style>
:root {{ --bg:#0f1117; --panel:#171a23; --borde:#232838; --texto:#e6e9f0; --suave:#9aa3b5; --acento:#5aa9ff; --codigo:#0b0d13; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; font-family:"Segoe UI",system-ui,-apple-system,sans-serif; background:var(--bg); color:var(--texto); display:flex; min-height:100vh; }}
nav {{ width:320px; flex:0 0 320px; background:var(--panel); border-right:1px solid var(--borde); padding:16px; overflow-y:auto; position:sticky; top:0; height:100vh; }}
nav h1 {{ font-size:15px; margin:0 0 4px; color:var(--acento); }}
nav .sub {{ font-size:12px; color:var(--suave); margin-bottom:14px; }}
#buscador {{ width:100%; padding:8px 10px; border-radius:8px; border:1px solid var(--borde); background:var(--codigo); color:var(--texto); margin-bottom:12px; }}
.nav-grupo {{ margin-bottom:14px; }}
.nav-titulo {{ font-size:11px; text-transform:uppercase; letter-spacing:.08em; color:var(--suave); margin:6px 0; }}
.nav-item {{ display:block; padding:6px 8px; border-radius:6px; color:var(--texto); text-decoration:none; font-size:13px; }}
.nav-item:hover {{ background:#1e2433; }}
.nav-item.activo {{ background:#22314f; color:var(--acento); }}
main {{ flex:1; padding:32px 44px; max-width:1000px; }}
.pagina {{ display:none; }}
.pagina.activa {{ display:block; }}
h1.titulo-pagina {{ font-size:26px; border-bottom:2px solid var(--borde); padding-bottom:8px; }}
h2 {{ font-size:20px; margin-top:28px; }}
h3 {{ font-size:17px; }}
p, li {{ line-height:1.65; font-size:14.5px; }}
blockquote {{ border-left:3px solid var(--acento); margin:12px 0; padding:6px 14px; background:#141a28; color:var(--suave); }}
pre.code {{ background:var(--codigo); border:1px solid var(--borde); border-radius:8px; padding:12px; overflow-x:auto; font-size:13px; }}
code {{ font-family:"Cascadia Code",Consolas,monospace; background:#1b2030; border-radius:4px; padding:1px 5px; font-size:13px; }}
pre code {{ background:none; padding:0; }}
table {{ border-collapse:collapse; margin:12px 0; width:100%; font-size:13.5px; }}
th, td {{ border:1px solid var(--borde); padding:7px 10px; text-align:left; }}
th {{ background:#1b2030; }}
a {{ color:var(--acento); }}
.idiomas {{ margin-bottom:10px; font-size:13px; }}
.idiomas a {{ margin-right:10px; }}
@media (max-width:900px) {{ nav {{ display:none; }} }}
</style>
</head>
<body>
<nav>
  <h1>cripsy_binary_API</h1>
  <div class="sub">API moderna para brókers de opciones binarias</div>
  <input id="buscador" type="search" placeholder="Buscar en la documentación…">
  <div class="idiomas"><a href="../es/index.html">Español</a><a href="../en/index.html">English</a></div>
  <div id="nav">{nav_html}</div>
</nav>
<main id="contenido">{contenido_html}</main>
<script>
var DOCS = [{busqueda_json}];
function mostrar(id) {{
  document.querySelectorAll('.pagina').forEach(function(s) {{ s.classList.remove('activa'); }});
  document.querySelectorAll('.nav-item').forEach(function(a) {{ a.classList.remove('activo'); }});
  var sec = document.getElementById(id);
  var enlace = document.querySelector('.nav-item[data-id="' + id + '"]');
  if (sec) {{ sec.classList.add('activa'); }}
  if (enlace) {{ enlace.classList.add('activo'); enlace.scrollIntoView({{ block:'nearest' }}); }}
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
window.addEventListener('hashchange', function() {{
  mostrar(location.hash.replace('#','') || '{primero}');
}});
var primero = '{primero}';
if (location.hash) {{ mostrar(location.hash.replace('#','')); }} else {{ mostrar(primero); }}
</script>
</body>
</html>
"#,
        lang = lang,
        nav_html = nav_html,
        contenido_html = contenido_html,
        busqueda_json = busqueda_json,
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
