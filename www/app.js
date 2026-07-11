// Cloudflare Worker: Single-file Deployment (workers.js)
// Serves static assets with COOP/COEP cross-origin headers, and provides CORS proxy routes.

const ASSETS_BASE_URL = "https://raw.githubusercontent.com/py7hon/yt-dlp.rs/main/www";

addEventListener('fetch', event => {
    event.respondWith(handleRequest(event.request));
});

async function handleRequest(request) {
    const url = new URL(request.url);
    const path = url.pathname;

    // 1. CORS Proxy Route: /proxy/https://...
    if (path.startsWith('/proxy/')) {
        return handleProxy(request, path.substring(7));
    }

    // 2. Serve main static files (inline or proxy fallback)
    if (path === '/' || path === '/index.html') {
        return serveHtml(request);
    }

    // --- FIX INI MBAH: Intercept ffmpeg-core & Proxy dari CDN ---
    // Karena file core terlalu besar dan sering gagal ditarik dari GitHub, 
    // kita ambil langsung dari CDN tapi tetap injek COOP/COEP headers di Worker.
    // Catatan: Ini menggunakan path v0.12.x. Jika ffmpeg.min.js kamu versi 0.11.x, 
    // hapus "/umd" dari URL di bawah.
    if (path === '/ffmpeg-core.js') {
        return fetchAndServeAsset(path, 'https://unpkg.com/@ffmpeg/core@0.12.6/dist/umd/ffmpeg-core.js');
    }
    if (path === '/ffmpeg-core.wasm') {
        return fetchAndServeAsset(path, 'https://unpkg.com/@ffmpeg/core@0.12.6/dist/umd/ffmpeg-core.wasm');
    }
    if (path === '/ffmpeg-core.worker.js') {
        return fetchAndServeAsset(path, 'https://unpkg.com/@ffmpeg/core@0.12.6/dist/umd/ffmpeg-core.worker.js');
    }
    // -------------------------------------------------------------

    // 3. Serve other static/binary assets by streaming from the source repository
    return fetchAndServeAsset(path);
}

// Serves the main index.html with cross-origin isolation headers
function serveHtml(request) {
    const origin = new URL(request.url).origin;
    const html = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>yt-dlp WASM - Browser YouTube Downloader</title>
    <link rel="stylesheet" href="index.css">
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;600;800&family=Plus+Jakarta+Sans:wght@300;400;600;700&display=swap" rel="stylesheet">
    <script src="ffmpeg.min.js"></script>
</head>
<body>
    <div class="glass-bg-accent-1"></div>
    <div class="glass-bg-accent-2"></div>
    
    <main class="app-container">
        <header class="app-header">
            <div class="logo-area">
                <span class="logo-badge">WASM</span>
                <h1>yt-dlp<span class="accent-text">.rs</span></h1>
            </div>
            <p class="subtitle">Download & merge YouTube video & audio entirely client-side in your browser using WebAssembly & ffmpeg.wasm</p>
        </header>

        <section class="card config-card">
            <div class="input-group-row">
                <div class="input-field-wrapper url-field">
                    <label for="video-url">YouTube Video URL</label>
                    <input type="text" id="video-url" placeholder="https://www.youtube.com/watch?v=dQw4w9WgXcQ" value="https://www.youtube.com/watch?v=dQw4w9WgXcQ">
                </div>
                  <div class="input-field-wrapper proxy-field">
                    <label for="cors-proxy">CORS Proxy URL <span class="info-tag">Optional</span></label>
                    <input type="text" id="cors-proxy" placeholder="${origin}/proxy/" value="${origin}/proxy/">
                </div>
                <button id="fetch-btn" class="btn btn-primary">
                    <span class="btn-text">Fetch Info</span>
                    <span class="loader-spinner hidden"></span>
                </button>
            </div>
            <div class="proxy-note">
                <p>⚡ <strong>Cloudflare Worker:</strong> The proxy has been auto-configured to route through this Worker instance.</p>
            </div>
        </section>

        <section id="results-card" class="card results-card hidden">
            <div class="video-meta-row">
                <div class="video-thumbnail-wrapper">
                    <img id="video-thumbnail" src="" alt="Video Thumbnail">
                </div>
                <div class="video-info-details">
                    <h2 id="video-title">Video Title</h2>
                    <div class="meta-tags">
                        <span id="video-duration" class="tag">Duration: --:--</span>
                        <span id="video-uploader" class="tag">Uploader: --</span>
                        <span id="video-views" class="tag">Views: --</span>
                    </div>
                    <p id="video-desc" class="video-desc-trunc"></p>
                </div>
            </div>

            <div class="formats-selection-row">
                <div class="format-select-column">
                    <h3>🎬 Video Stream <span class="optional-lbl">(Visuals only or combined)</span></h3>
                    <div class="format-list-wrapper">
                        <select id="video-format-select" size="6" class="format-select-list">
                        </select>
                    </div>
                </div>

                <div class="format-select-column">
                    <h3>🎵 Audio Stream <span class="optional-lbl">(Sound only)</span></h3>
                    <div class="format-list-wrapper">
                        <select id="audio-format-select" size="6" class="format-select-list">
                        </select>
                    </div>
                </div>
            </div>

            <div class="action-footer">
                <div class="mode-selector-wrapper">
                    <label class="mode-label">⬇️ Download Mode</label>
                    <div class="mode-buttons" id="mode-buttons">
                        <button class="mode-btn active" data-mode="merge" id="mode-merge">🎬 Video + Audio (MKV)</button>
                        <button class="mode-btn" data-mode="mp3" id="mode-mp3">🎵 Audio Only (MP3)</button>
                        <button class="mode-btn" data-mode="video-only" id="mode-video-only">📹 Video Only</button>
                        <button class="mode-btn" data-mode="audio-only" id="mode-audio-only">🔊 Audio Only (raw)</button>
                    </div>
                </div>
                <button id="download-btn" class="btn btn-success">
                    <span class="btn-text">⬇ Download</span>
                    <span class="loader-spinner hidden"></span>
                </button>
            </div>
        </section>

        <section id="progress-card" class="card progress-card hidden">
            <h3>⏳ Operation Progress</h3>
            
            <div class="progress-bars-container">
                <div id="video-progress-row" class="progress-row hidden">
                    <div class="progress-meta">
                        <span class="progress-title">🎬 Video Stream Download</span>
                        <span id="video-progress-percent" class="progress-percent">0%</span>
                    </div>
                    <div class="progress-bar-track">
                        <div id="video-progress-bar" class="progress-bar-fill" style="width: 0%"></div>
                    </div>
                </div>

                <div id="audio-progress-row" class="progress-row hidden">
                    <div class="progress-meta">
                        <span class="progress-title">🎵 Audio Stream Download</span>
                        <span id="audio-progress-percent" class="progress-percent">0%</span>
                    </div>
                    <div class="progress-bar-track">
                        <div id="audio-progress-bar" class="progress-bar-fill" style="width: 0%"></div>
                    </div>
                </div>

                <div id="merge-progress-row" class="progress-row hidden">
                    <div class="progress-meta">
                        <span class="progress-title">⚙️ WebAssembly Muxing (ffmpeg.wasm)</span>
                        <span id="merge-progress-status" class="progress-status">Muxing...</span>
                    </div>
                    <div class="progress-bar-track progress-striped">
                        <div id="merge-progress-bar" class="progress-bar-fill progress-animated" style="width: 100%"></div>
                    </div>
                </div>
            </div>

            <div class="log-console-wrapper">
                <h4>Console Output Logger</h4>
                <pre id="log-console" class="log-console">Initialized WASM Downloader. Waiting for download triggers...</pre>
            </div>
        </section>
    </main>

    <script type="module" src="app.js"></script>
</body>
</html>`;

    return new Response(html, {
        headers: {
            'Content-Type': 'text/html; charset=utf-8',
            'Cross-Origin-Opener-Policy': 'same-origin',
            'Cross-Origin-Embedder-Policy': 'require-corp',
            'Cross-Origin-Resource-Policy': 'cross-origin'
        }
    });
}

// Proxies/fetches static resources, now supports optional overrideUrl for CDNs
async function fetchAndServeAsset(path, overrideUrl = null) {
    // Jika overrideUrl diset (untuk file FFmpeg), pakai itu. Kalau tidak, ambil dari GitHub.
    const assetUrl = overrideUrl || `${ASSETS_BASE_URL}${path}`;
    const response = await fetch(assetUrl);

    if (!response.ok) {
        return new Response(`Asset ${path} not found. Build or configuration error.`, { status: 404 });
    }

    const headers = new Headers(response.headers);
    headers.set('Cross-Origin-Opener-Policy', 'same-origin');
    headers.set('Cross-Origin-Embedder-Policy', 'require-corp');
    headers.set('Cross-Origin-Resource-Policy', 'cross-origin');
    headers.delete('content-security-policy');

    if (path.endsWith('.js')) {
        headers.set('Content-Type', 'text/javascript');
    } else if (path.endsWith('.wasm')) {
        headers.set('Content-Type', 'application/wasm');
    } else if (path.endsWith('.css')) {
        headers.set('Content-Type', 'text/css');
    }

    return new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers: headers
    });
}

// Proxy Core Module
async function handleProxy(request, targetUrlStr) {
    if (request.method === 'OPTIONS') {
        return new Response(null, {
            headers: {
                'Access-Control-Allow-Origin': '*',
                'Access-Control-Allow-Methods': 'GET, POST, OPTIONS, PUT, PATCH, DELETE',
                'Access-Control-Allow-Headers': 'Content-Type, X-YouTube-Client-Name, X-YouTube-Client-Version, X-Cors-User-Agent, Range',
                'Access-Control-Expose-Headers': 'Content-Length, Content-Range, Accept-Ranges',
                'Access-Control-Allow-Credentials': 'true',
                'Cross-Origin-Resource-Policy': 'cross-origin'
            }
        });
    }

    let cleanUrlStr = decodeURIComponent(targetUrlStr);
    if (cleanUrlStr.startsWith('https:/') && !cleanUrlStr.startsWith('https://')) {
        cleanUrlStr = 'https://' + cleanUrlStr.substring(7);
    } else if (cleanUrlStr.startsWith('http:/') && !cleanUrlStr.startsWith('http://')) {
        cleanUrlStr = 'http://' + cleanUrlStr.substring(6);
    }

    try {
        const targetUrl = new URL(cleanUrlStr);
        const headers = new Headers(request.headers);

        if (headers.has('x-cors-user-agent')) {
            headers.set('user-agent', headers.get('x-cors-user-agent'));
            headers.delete('x-cors-user-agent');
        }

        if (targetUrl.hostname.endsWith('youtube.com') || targetUrl.hostname.endsWith('googlevideo.com')) {
            headers.set('origin', 'https://www.youtube.com');
            headers.set('referer', 'https://www.youtube.com/');
        }

        if (targetUrl.hostname.endsWith('googlevideo.com')) {
            headers.set('user-agent', 'com.google.ios.youtube/21.02.3 (iPhone16,2; U; CPU iPhone OS 18_3_2 like Mac OS X)');
            headers.delete('accept-encoding');
        }

        headers.delete('host');
        headers.delete('cookie');
        for (const [key] of headers.entries()) {
            if (key.startsWith('sec-')) {
                headers.delete(key);
            }
        }

        if (targetUrl.pathname.endsWith('/youtubei/v1/player')) {
            try {
                const cookieRes = await fetch('https://www.youtube.com/', {
                    headers: {
                        'user-agent': headers.get('user-agent') || 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'
                    }
                });
                const setCookies = cookieRes.headers.get('set-cookie');
                if (setCookies) {
                    const cookieStr = setCookies.split(',')
                        .map(c => c.split(';')[0].trim())
                        .filter(c => c && !c.includes('path=') && !c.includes('domain='))
                        .join('; ');
                    if (cookieStr) {
                        headers.set('cookie', cookieStr);
                    }
                }
            } catch (err) { }
        }

        const targetResponse = await fetch(targetUrl.href, {
            method: request.method,
            headers: headers,
            body: request.method !== 'GET' && request.method !== 'HEAD' ? await request.arrayBuffer() : undefined,
            redirect: 'follow'
        });

        const responseHeaders = new Headers(targetResponse.headers);

        responseHeaders.set('Access-Control-Allow-Origin', '*');
        responseHeaders.set('Access-Control-Allow-Methods', 'GET, POST, OPTIONS, PUT, PATCH, DELETE');
        responseHeaders.set('Access-Control-Allow-Headers', 'Content-Type, X-YouTube-Client-Name, X-YouTube-Client-Version, X-Cors-User-Agent, Range');
        responseHeaders.set('Access-Control-Expose-Headers', 'Content-Length, Content-Range, Accept-Ranges');
        responseHeaders.set('Access-Control-Allow-Credentials', 'true');
        responseHeaders.set('Cross-Origin-Resource-Policy', 'cross-origin');

        responseHeaders.delete('cross-origin-opener-policy');
        responseHeaders.delete('cross-origin-embedder-policy');
        responseHeaders.delete('content-security-policy');

        return new Response(targetResponse.body, {
            status: targetResponse.status,
            statusText: targetResponse.statusText,
            headers: responseHeaders
        });

    } catch (err) {
        return new Response(`Proxy Error: ${err.message}`, { status: 500 });
    }
}