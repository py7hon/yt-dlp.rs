const http = require('http');
const fs = require('fs');
const path = require('path');

const PORT = 3000;
const WWW_DIR = __dirname;

const MIME_TYPES = {
    '.html': 'text/html',
    '.css': 'text/css',
    '.js': 'text/javascript',
    '.wasm': 'application/wasm',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.gif': 'image/gif',
    '.svg': 'image/svg+xml',
    '.ico': 'image/x-icon'
};

const server = http.createServer((req, res) => {
    // Inject Cross-Origin Isolation headers required for SharedArrayBuffer (ffmpeg.wasm)
    res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
    res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
    res.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');

    // Parse the file path and remove query parameters
    let requestPath = req.url.split('?')[0];
    if (requestPath === '/') {
        requestPath = '/index.html';
    }

    const filePath = path.join(WWW_DIR, requestPath);
    
    // Security check: ensure requests stay within the www directory
    if (!filePath.startsWith(WWW_DIR)) {
        res.writeHead(403, { 'Content-Type': 'text/plain' });
        res.end('403 Forbidden');
        return;
    }

    fs.readFile(filePath, (err, data) => {
        if (err) {
            if (err.code === 'ENOENT') {
                res.writeHead(404, { 'Content-Type': 'text/plain' });
                res.end('404 Not Found');
            } else {
                res.writeHead(500, { 'Content-Type': 'text/plain' });
                res.end(`500 Server Error: ${err.code}`);
            }
            return;
        }

        const ext = path.extname(filePath).toLowerCase();
        res.writeHead(200, { 'Content-Type': MIME_TYPES[ext] || 'application/octet-stream' });
        res.end(data);
    });
});

server.on('error', (e) => {
    if (e.code === 'EADDRINUSE') {
        console.error(`\n[ERROR] Port ${PORT} is already in use by another server!`);
        console.error(`Please stop your other dev server before running this one.`);
        process.exit(1);
    }
});

server.listen(PORT, () => {
    console.log(`====================================================`);
    console.log(` yt-dlp.rs Web Server running at: http://localhost:${PORT}/`);
    console.log(` COOP & COEP Cross-Origin Isolation headers: ACTIVE`);
    console.log(`====================================================`);
});
