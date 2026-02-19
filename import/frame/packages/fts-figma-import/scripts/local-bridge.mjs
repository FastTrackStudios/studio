import { createServer } from 'node:http';
import { writeFile } from 'node:fs/promises';

const PORT = Number(process.env.FTS_FIGMA_BRIDGE_PORT || 43123);
const HOST = process.env.FTS_FIGMA_BRIDGE_HOST || '127.0.0.1';
const OUT_PATH = process.env.FTS_FIGMA_OUT_PATH || '/tmp/fts-figma-export.json';

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type',
};

function sendJson(res, statusCode, payload) {
  res.writeHead(statusCode, {
    'content-type': 'application/json',
    ...CORS_HEADERS,
  });
  res.end(JSON.stringify(payload));
}

const server = createServer(async (req, res) => {
  if (req.method === 'OPTIONS') {
    res.writeHead(204, CORS_HEADERS);
    res.end();
    return;
  }

  if (req.method !== 'POST' || req.url !== '/figma-export') {
    sendJson(res, 404, { ok: false, error: 'not found' });
    return;
  }

  try {
    const chunks = [];
    for await (const chunk of req) {
      chunks.push(chunk);
    }

    const body = Buffer.concat(chunks);
    if (body.length === 0) {
      sendJson(res, 400, { ok: false, error: 'empty payload' });
      return;
    }

    JSON.parse(body.toString('utf8'));
    await writeFile(OUT_PATH, body);

    sendJson(res, 200, {
      ok: true,
      path: OUT_PATH,
      bytes: body.length,
    });
  } catch (error) {
    sendJson(res, 500, {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
});

server.listen(PORT, HOST, () => {
  console.log(`[fts-figma-bridge] listening on http://${HOST}:${PORT}`);
  console.log(`[fts-figma-bridge] writing to ${OUT_PATH}`);
});
