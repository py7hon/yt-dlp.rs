const http = require('http');
const https = require('https');

const PORT = 8080;

const server = http.createServer((req, res) => {
    // Immediate diagnostics log to verify browser-to-proxy connection
    console.log(`\n[CORS Proxy] Connection received: ${req.method} ${req.url}`);

    // Add CORS & CORP Headers matching both preflight and actual response requirements
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS, PUT, PATCH, DELETE');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type, X-YouTube-Client-Name, X-YouTube-Client-Version, X-Cors-User-Agent, Range');
    res.setHeader('Access-Control-Expose-Headers', 'Content-Length, Content-Range, Accept-Ranges');
    res.setHeader('Access-Control-Allow-Credentials', 'true');
    res.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');

    // Handle Preflight OPTIONS request
    if (req.method === 'OPTIONS') {
        console.log(`[CORS Proxy] Handled OPTIONS preflight request`);
        res.writeHead(200);
        res.end();
        return;
    }

    // Parse the target URL from the path (e.g. /https://www.youtube.com/...)
    let targetUrlStr = req.url.substring(1);
    
    // Fix browser-collapsed slashes (e.g., https:/www.youtube.com -> https://www.youtube.com)
    if (targetUrlStr.startsWith('https:/') && !targetUrlStr.startsWith('https://')) {
        targetUrlStr = 'https://' + targetUrlStr.substring(7);
    } else if (targetUrlStr.startsWith('http:/') && !targetUrlStr.startsWith('http://')) {
        targetUrlStr = 'http://' + targetUrlStr.substring(6);
    }

    if (!targetUrlStr.startsWith('http://') && !targetUrlStr.startsWith('https://')) {
        console.log(`[CORS Proxy] Ignored non-HTTP target path: ${req.url}`);
        res.writeHead(400, { 'Content-Type': 'text/plain' });
        res.end('Error: Target URL must start with http:// or https://');
        return;
    }

    try {
        const targetUrl = new URL(targetUrlStr);
        const headers = { ...req.headers };
        
        // Extract and map custom proxy headers
        if (headers['x-cors-user-agent']) {
            headers['user-agent'] = headers['x-cors-user-agent'];
            delete headers['x-cors-user-agent'];
        }

        // Set clean referrer/origin headers for YouTube target domains
        if (targetUrl.hostname.endsWith('youtube.com') || targetUrl.hostname.endsWith('googlevideo.com')) {
            headers['origin'] = 'https://www.youtube.com';
            headers['referer'] = 'https://www.youtube.com/';
        }

        // For CDN download requests, force iOS YouTube UA and disable compression.
        // Browsers silently drop User-Agent via fetch(); this proxy-level override ensures
        // googlevideo.com sees a valid YouTube client UA and returns uncompressed data.
        if (targetUrl.hostname.endsWith('googlevideo.com')) {
            headers['user-agent'] = 'com.google.ios.youtube/21.02.3 (iPhone16,2; U; CPU iPhone OS 18_3_2 like Mac OS X)';
            delete headers['accept-encoding']; // force uncompressed response
        }

        // Clean browser host and cookie overrides
        delete headers.host;
        delete headers.cookie;

        // Strip all browser-specific security/fetch metadata headers (sec-*)
        for (const key in headers) {
            if (key.startsWith('sec-')) {
                delete headers[key];
            }
        }

        console.log(`[CORS Proxy] Forwarding: ${req.method} -> ${targetUrl.href}`);

        const makeRequest = (currentUrl, redirectCount = 0) => {
            if (redirectCount > 5) {
                res.writeHead(502, { 'Content-Type': 'text/plain' });
                res.end('Error: Too many redirects');
                return;
            }

            const options = {
                hostname: currentUrl.hostname,
                port: currentUrl.port || (currentUrl.protocol === 'https:' ? 443 : 80),
                path: currentUrl.pathname + currentUrl.search,
                method: req.method,
                headers: headers
            };

            const proxyReq = (currentUrl.protocol === 'https:' ? https : http).request(options, (proxyRes) => {
                const statusCode = proxyRes.statusCode;
                
                // If it is a redirect, follow it server-side
                if ([301, 302, 303, 307, 308].includes(statusCode) && proxyRes.headers.location) {
                    try {
                        const redirectUrl = new URL(proxyRes.headers.location, currentUrl.href);
                        console.log(`[CORS Proxy] Redirected (${statusCode}) -> following to: ${redirectUrl.href}`);
                        makeRequest(redirectUrl, redirectCount + 1);
                        return;
                    } catch (err) {
                        console.error(`[CORS Proxy] Failed to parse redirect location: ${proxyRes.headers.location}`);
                    }
                }

                console.log(`[CORS Proxy] Target responded: HTTP ${statusCode} for ${currentUrl.pathname}`);
                
                const responseHeaders = { ...proxyRes.headers };
                
                // Force CORS and CORP headers on target response
                responseHeaders['access-control-allow-origin'] = '*';
                responseHeaders['access-control-allow-methods'] = 'GET, POST, OPTIONS, PUT, PATCH, DELETE';
                responseHeaders['access-control-allow-headers'] = 'Content-Type, X-YouTube-Client-Name, X-YouTube-Client-Version, X-Cors-User-Agent, Range';
                responseHeaders['access-control-expose-headers'] = 'Content-Length, Content-Range, Accept-Ranges';
                responseHeaders['access-control-allow-credentials'] = 'true';
                responseHeaders['cross-origin-resource-policy'] = 'cross-origin';

                // Strip YouTube's cross-origin isolation policies that block third-party browser reading
                delete responseHeaders['cross-origin-opener-policy'];
                delete responseHeaders['cross-origin-embedder-policy'];
                delete responseHeaders['content-security-policy'];

                res.writeHead(statusCode, responseHeaders);
                proxyRes.pipe(res);
            });

            proxyReq.on('error', (e) => {
                console.error(`[CORS Proxy] Target request error: ${e.message}`);
                res.writeHead(500, { 'Content-Type': 'text/plain' });
                res.end(`Proxy error: ${e.message}`);
            });

            // Only pipe incoming body for the initial request (if method matches)
            if (redirectCount === 0 && (req.method === 'POST' || req.method === 'PUT' || req.method === 'PATCH')) {
                req.pipe(proxyReq);
            } else {
                proxyReq.end();
            }
        };

        makeRequest(targetUrl);
    } catch (err) {
        console.error(`[CORS Proxy] Setup error: ${err.message}`);
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end(`Proxy setup error: ${err.message}`);
    }
});

// Port collision error listener
server.on('error', (e) => {
    if (e.code === 'EADDRINUSE') {
        console.error(`\n[ERROR] Port ${PORT} is already occupied by another program!`);
        console.error('Please terminate the other application or release port 8080 to proceed.');
        process.exit(1);
    }
});

server.listen(PORT, () => {
    console.log(`CORS Proxy listening on: http://localhost:${PORT}`);
});
