// Local plain-HTTP crates mirror proxy.
//
// Why: this machine's native TLS stack (schannel, used by cargo/curl/.NET)
// is broken (SEC_E_NO_CREDENTIALS), but Node's OpenSSL stack works. Cargo can
// be pointed at a `sparse+http://` registry, so this server accepts plain HTTP
// on loopback and forwards each request to the real upstream HTTPS mirror
// (rsproxy.cn) using Node fetch (undici/OpenSSL), following redirects.
//
// It also rewrites the upstream index `config.json` `dl` field so crate
// downloads also come back through this proxy over plain HTTP.
//
// Usage: node scripts/registry-proxy.mjs [--port 8765]

import http from "node:http";

const PORT = Number(process.argv[process.argv.indexOf("--port") + 1] || 8765);
const UPSTREAM = "https://rsproxy.cn";

async function upstream(pathname, { text = false } = {}) {
  const url = UPSTREAM + pathname;
  const resp = await fetch(url, { redirect: "follow" });
  const body = text ? await resp.text() : Buffer.from(await resp.arrayBuffer());
  return { status: resp.status, contentType: resp.headers.get("content-type"), body };
}

async function handle(req, res) {
  const { pathname } = new URL(req.url, `http://127.0.0.1:${PORT}`);

  // Sparse index: upstream path is `/index/...`
  if (pathname.startsWith("/index/")) {
    const isConfig = pathname.endsWith("/config.json");
    const { status, contentType, body } = await upstream(pathname, {
      text: isConfig,
    });
    let out = Buffer.isBuffer(body) ? body : Buffer.from(body, "utf8");
    if (isConfig) {
      try {
        const json = JSON.parse(body);
        json.dl = `http://127.0.0.1:${PORT}/api/v1/crates/{crate}/{version}/download`;
        out = Buffer.from(JSON.stringify(json), "utf8");
      } catch (e) {
        res.writeHead(502);
        res.end("proxy: bad index config: " + e.message);
        return;
      }
    }
    res.writeHead(status, { "content-type": contentType || "application/json" });
    res.end(out);
    return;
  }

  // Crate downloads: `/api/v1/crates/{crate}/{version}/download`
  if (pathname.startsWith("/api/v1/crates/")) {
    const { status, body } = await upstream(pathname);
    res.writeHead(status, {
      "content-type": "application/octet-stream",
      "content-length": body.length,
    });
    res.end(body);
    return;
  }

  res.writeHead(404);
  res.end("proxy: not found");
}

const server = http.createServer((req, res) => {
  handle(req, res).catch((err) => {
    res.writeHead(502);
    res.end("proxy error: " + err.message);
  });
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`registry-proxy listening on http://127.0.0.1:${PORT} -> ${UPSTREAM}`);
});
