//! Landing page service — the service catalog served at the root path.
//!
//! Renders an HTML index listing every OGC service the server exposes, with
//! a link to each service's GetCapabilities document. QGIS Server serves the
//! same catalog when a request arrives without a `SERVICE` parameter.

/// A single entry in the service catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    /// Service abbreviation (e.g. "WMS").
    pub name: &'static str,
    /// Human-readable title.
    pub title: &'static str,
    /// Path segment the service is mounted at.
    pub path: &'static str,
    /// Specification versions implemented.
    pub versions: &'static [&'static str],
    /// Operations the service supports.
    pub operations: &'static [&'static str],
}

/// Returns the catalog of services published by this server.
pub fn service_catalog() -> &'static [ServiceEntry] {
    const SERVICES: &[ServiceEntry] = &[
        ServiceEntry {
            name: "WMS",
            title: "Web Map Service",
            path: "/wms",
            versions: &["1.3.0", "1.1.1"],
            operations: &["GetCapabilities", "GetMap", "GetFeatureInfo", "GetLegendGraphic"],
        },
        ServiceEntry {
            name: "WFS",
            title: "Web Feature Service",
            path: "/wfs",
            versions: &["2.0.0"],
            operations: &["GetCapabilities", "DescribeFeatureType", "GetFeature"],
        },
        ServiceEntry {
            name: "WCS",
            title: "Web Coverage Service",
            path: "/wcs",
            versions: &["2.0.1"],
            operations: &["GetCapabilities", "DescribeCoverage", "GetCoverage"],
        },
        ServiceEntry {
            name: "WMTS",
            title: "Web Map Tile Service",
            path: "/wmts",
            versions: &["1.0.0"],
            operations: &["GetCapabilities", "GetTile", "GetFeatureInfo"],
        },
    ];
    SERVICES
}

/// Returns the catalog entry for a service name, matched case-insensitively.
pub fn find_service(name: &str) -> Option<&'static ServiceEntry> {
    service_catalog()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
}

/// Builds the GetCapabilities link for a service entry.
pub fn capabilities_link(service: &ServiceEntry, base_url: &str) -> String {
    format!(
        "{base}{path}?SERVICE={name}&amp;REQUEST=GetCapabilities",
        base = base_url.trim_end_matches('/'),
        path = service.path,
        name = service.name,
    )
}

/// Escapes HTML text content.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Renders the landing page HTML for the given base URL.
pub fn render(base_url: &str, host: &str, port: u16) -> String {
    let mut services = String::new();

    for service in service_catalog() {
        let versions = service
            .versions
            .iter()
            .map(|v| format!("<li>{}</li>", html_escape(v)))
            .collect::<Vec<_>>()
            .join("");
        let operations = service
            .operations
            .iter()
            .map(|op| format!("<code>{}</code>", html_escape(op)))
            .collect::<Vec<_>>()
            .join(", ");

        services.push_str(&format!(
            r#"    <tr>
      <td><strong>{name}</strong></td>
      <td>{title}</td>
      <td><a href="{link}">GetCapabilities</a></td>
      <td>{versions}</td>
      <td>{operations}</td>
    </tr>
"#,
            name = service.name,
            title = html_escape(service.title),
            link = capabilities_link(service, base_url),
            versions = versions,
            operations = operations,
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <title>QGIS Server</title>
  <style>
    body {{ font-family: sans-serif; margin: 2em; }}
    table {{ border-collapse: collapse; }}
    th, td {{ border: 1px solid #999; padding: 0.4em 0.8em; text-align: left; }}
    code {{ background: #f0f0f0; padding: 0 0.2em; }}
    ul {{ margin: 0; padding-left: 1.2em; display: inline-block; }}
  </style>
</head>
<body>
  <h1>QGIS Server</h1>
  <p>Service catalog for the QGIS Rust server at <code>{host}:{port}</code>.</p>
  <table>
    <thead>
      <tr>
        <th>Service</th>
        <th>Description</th>
        <th>Capabilities</th>
        <th>Versions</th>
        <th>Operations</th>
      </tr>
    </thead>
    <tbody>
{services}    </tbody>
  </table>
  <h2>Endpoint</h2>
  <p>All services are also reachable through the FastCGI endpoint
     <code>{base_url}/cgi-bin/qgis_mapserv.fcgi</code>.</p>
</body>
</html>
"#,
        host = html_escape(host),
        port = port,
        base_url = html_escape(base_url.trim_end_matches('/')),
        services = services,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_four_services() {
        assert_eq!(service_catalog().len(), 4);
        assert_eq!(service_catalog()[0].name, "WMS");
        assert_eq!(service_catalog()[2].name, "WCS");
        assert_eq!(service_catalog()[3].name, "WMTS");
    }

    #[test]
    fn catalog_entry_fields() {
        let wcs = find_service("WCS").unwrap();
        assert_eq!(wcs.title, "Web Coverage Service");
        assert_eq!(wcs.path, "/wcs");
        assert_eq!(wcs.versions, &["2.0.1"]);
        assert!(wcs.operations.contains(&"DescribeCoverage"));
        assert!(wcs.operations.contains(&"GetCoverage"));
    }

    #[test]
    fn wmts_operations_include_tile_and_featureinfo() {
        let wmts = find_service("WMTS").unwrap();
        assert!(wmts.operations.contains(&"GetTile"));
        assert!(wmts.operations.contains(&"GetFeatureInfo"));
        assert_eq!(wmts.versions, &["1.0.0"]);
    }

    #[test]
    fn find_service_is_case_insensitive() {
        assert_eq!(find_service("wmts").unwrap().name, "WMTS");
        assert_eq!(find_service("Wmts").unwrap().name, "WMTS");
        assert!(find_service("nope").is_none());
    }

    #[test]
    fn capabilities_link_builds_query() {
        let wcs = find_service("WCS").unwrap();
        let link = capabilities_link(wcs, "http://localhost:8080");
        assert_eq!(link, "http://localhost:8080/wcs?SERVICE=WCS&amp;REQUEST=GetCapabilities");
    }

    #[test]
    fn capabilities_link_trims_trailing_slash() {
        let wms = find_service("WMS").unwrap();
        let link = capabilities_link(wms, "http://localhost:8080/");
        assert!(link.starts_with("http://localhost:8080/wms?"));
    }

    #[test]
    fn render_includes_all_services() {
        let html = render("http://localhost:8080", "127.0.0.1", 8080);
        for service in service_catalog() {
            assert!(html.contains(service.name), "missing {}", service.name);
        }
    }

    #[test]
    fn render_includes_links() {
        let html = render("http://localhost:8080", "127.0.0.1", 8080);
        assert!(html.contains("http://localhost:8080/wms?SERVICE=WMS"));
        assert!(html.contains("http://localhost:8080/wcs?SERVICE=WCS"));
        assert!(html.contains("http://localhost:8080/wmts?SERVICE=WMTS"));
    }

    #[test]
    fn render_is_html_document() {
        let html = render("http://localhost:8080", "127.0.0.1", 8080);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>QGIS Server</title>"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn render_shows_host_and_port() {
        let html = render("http://localhost:8080", "0.0.0.0", 9090);
        assert!(html.contains("0.0.0.0"));
        assert!(html.contains("9090"));
    }

    #[test]
    fn render_lists_operations() {
        let html = render("http://localhost:8080", "127.0.0.1", 8080);
        assert!(html.contains("<code>GetTile</code>"));
        assert!(html.contains("<code>DescribeCoverage</code>"));
        assert!(html.contains("<code>GetMap</code>"));
    }

    #[test]
    fn render_mentions_fcgi_endpoint() {
        let html = render("http://localhost:8080", "127.0.0.1", 8080);
        assert!(html.contains("/cgi-bin/qgis_mapserv.fcgi"));
    }

    #[test]
    fn html_escape_special_chars() {
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("<x>"), "&lt;x&gt;");
        assert_eq!(html_escape("\""), "&quot;");
    }
}