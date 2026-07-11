// Cloudflare Pages Function (functions/proxy.js)
// Handles target routing for CORS proxy requests

export async function onRequest(context) {
  const request = context.request;
  const url = new URL(request.url);
  
  // Extract target URL from pathname (e.g. /proxy/https://...)
  const prefix = "/proxy/";
  const targetIndex = url.pathname.indexOf(prefix);
  if (targetIndex === -1) {
    return new Response("Invalid proxy request format.", { status: 400 });
  }
  
  const targetUrlStr = url.pathname.substring(targetIndex + prefix.length) + url.search;

  // Handle CORS Preflight OPTIONS
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

    // Map custom CORS User-Agent back
    if (headers.has('x-cors-user-agent')) {
      headers.set('user-agent', headers.get('x-cors-user-agent'));
      headers.delete('x-cors-user-agent');
    }

    // Standardize YouTube / GoogleVideo CDN headers
    if (targetUrl.hostname.endsWith('youtube.com') || targetUrl.hostname.endsWith('googlevideo.com')) {
      headers.set('origin', 'https://www.youtube.com');
      headers.set('referer', 'https://www.youtube.com/');
    }

    // Force Android/iOS headers for media CDN playback routes
    if (targetUrl.hostname.endsWith('googlevideo.com')) {
      headers.set('user-agent', 'com.google.ios.youtube/21.02.3 (iPhone16,2; U; CPU iPhone OS 18_3_2 like Mac OS X)');
      headers.delete('accept-encoding'); // Bypass browser-side GZIP issues
    }

    headers.delete('host');
    headers.delete('cookie');
    for (const [key] of headers.entries()) {
      if (key.startsWith('sec-')) {
        headers.delete(key);
      }
    }

    const targetResponse = await fetch(targetUrl.href, {
      method: request.method,
      headers: headers,
      body: request.method !== 'GET' && request.method !== 'HEAD' ? await request.arrayBuffer() : undefined,
      redirect: 'follow'
    });

    const responseHeaders = new Headers(targetResponse.headers);

    // Apply CORS/CORP headers
    responseHeaders.set('Access-Control-Allow-Origin', '*');
    responseHeaders.set('Access-Control-Allow-Methods', 'GET, POST, OPTIONS, PUT, PATCH, DELETE');
    responseHeaders.set('Access-Control-Allow-Headers', 'Content-Type, X-YouTube-Client-Name, X-YouTube-Client-Version, X-Cors-User-Agent, Range');
    responseHeaders.set('Access-Control-Expose-Headers', 'Content-Length, Content-Range, Accept-Ranges');
    responseHeaders.set('Access-Control-Allow-Credentials', 'true');
    responseHeaders.set('Cross-Origin-Resource-Policy', 'cross-origin');

    // Remove third-party COOP/COEP headers
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
