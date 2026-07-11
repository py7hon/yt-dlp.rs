import init, { wasm_extract_info, wasm_download_format } from "./pkg/yt_dlp.js";

// DOM elements
const videoUrlInput = document.getElementById("video-url");
const corsProxyInput = document.getElementById("cors-proxy");
const fetchBtn = document.getElementById("fetch-btn");
const downloadBtn = document.getElementById("download-btn");
const resultsCard = document.getElementById("results-card");
const progressCard = document.getElementById("progress-card");

const videoThumbnail = document.getElementById("video-thumbnail");
const videoTitle = document.getElementById("video-title");
const videoDuration = document.getElementById("video-duration");
const videoUploader = document.getElementById("video-uploader");
const videoViews = document.getElementById("video-views");
const videoDesc = document.getElementById("video-desc");

const videoFormatSelect = document.getElementById("video-format-select");
const audioFormatSelect = document.getElementById("audio-format-select");

const videoProgressRow = document.getElementById("video-progress-row");
const videoProgressPercent = document.getElementById("video-progress-percent");
const videoProgressBar = document.getElementById("video-progress-bar");

const audioProgressRow = document.getElementById("audio-progress-row");
const audioProgressPercent = document.getElementById("audio-progress-percent");
const audioProgressBar = document.getElementById("audio-progress-bar");

const mergeProgressRow = document.getElementById("merge-progress-row");
const mergeProgressStatus = document.getElementById("merge-progress-status");
const logConsoleEl = document.getElementById("log-console");

// Mode buttons
const modeButtons = document.querySelectorAll(".mode-btn");
let downloadMode = "merge"; // merge | mp3 | video-only | audio-only

modeButtons.forEach(btn => {
    btn.addEventListener("click", () => {
        modeButtons.forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        downloadMode = btn.dataset.mode;

        // Show/hide selects based on mode
        const videoCol = videoFormatSelect.closest(".format-select-column");
        const audioCol = audioFormatSelect.closest(".format-select-column");
        if (downloadMode === "mp3" || downloadMode === "audio-only") {
            videoCol.style.opacity = "0.35";
            videoCol.style.pointerEvents = "none";
            audioCol.style.opacity = "1";
            audioCol.style.pointerEvents = "";
        } else if (downloadMode === "video-only") {
            audioCol.style.opacity = "0.35";
            audioCol.style.pointerEvents = "none";
            videoCol.style.opacity = "1";
            videoCol.style.pointerEvents = "";
        } else {
            videoCol.style.opacity = "1";
            videoCol.style.pointerEvents = "";
            audioCol.style.opacity = "1";
            audioCol.style.pointerEvents = "";
        }

        // Update button label
        const labels = {
            "merge": "⬇ Download & Mux (MKV)",
            "mp3": "🎵 Download as MP3",
            "video-only": "📹 Download Video Only",
            "audio-only": "🔊 Download Audio Only",
        };
        downloadBtn.querySelector(".btn-text").textContent = labels[downloadMode] || "⬇ Download";
    });
});

// App State
let currentVideoInfo = null;
let ffmpeg = null;
let ffmpegLoaded = false;

// Helpers
function logConsole(message) {
    const timestamp = new Date().toLocaleTimeString();
    logConsoleEl.textContent += `\n[${timestamp}] ${message}`;
    logConsoleEl.scrollTop = logConsoleEl.scrollHeight;
}

function sanitizeFilename(name) {
    return name.replace(/[\\/:*?"<>|]/g, "_").trim();
}

function formatBytes(bytes) {
    if (!bytes) return "unknown size";
    const UNITS = ["B", "KiB", "MiB", "GiB"];
    let size = bytes, idx = 0;
    while (size >= 1024 && idx < UNITS.length - 1) { size /= 1024; idx++; }
    return `${size.toFixed(2)} ${UNITS[idx]}`;
}

function formatDuration(secs) {
    if (!secs) return "00:00";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    const pad = n => String(n).padStart(2, "0");
    return h > 0 ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

// Initialize WASM Downloader
async function start() {
    logConsole(`App Origin: ${window.location.origin}`);

    // Auto-detect hosted/Worker proxy setup
    const isLocalhostDev = window.location.hostname === "localhost" && window.location.port === "3000";
    const isLocalIPDev = window.location.hostname === "127.0.0.1" && window.location.port === "3000";
    if (!isLocalhostDev && !isLocalIPDev) {
        corsProxyInput.value = `${window.location.origin}/proxy`;
        logConsole(`Auto-configured CORS proxy target to: ${corsProxyInput.value}`);
    }

    try {
        logConsole("Initializing Rust WebAssembly downloader module...");
        await init();
        logConsole("Rust WebAssembly module successfully initialized.");
    } catch (e) {
        logConsole(`ERROR: Failed to initialize WASM: ${e}`);
    }
}

// Load ffmpeg.wasm
async function toBlobURL(url, mimeType) {
    const resp = await fetch(url);
    const blob = await resp.blob();
    const finalBlob = new Blob([blob], { type: mimeType });
    return URL.createObjectURL(finalBlob);
}

// Load ffmpeg.wasm
async function ensureFFmpegLoaded() {
    if (ffmpegLoaded) return;

    logConsole("Initializing ffmpeg.wasm framework...");
    const { FFmpeg } = window.FFmpegWASM || window.FFmpegWasm || {};

    ffmpeg = new FFmpeg();
    ffmpeg.on("log", ({ message }) => logConsole(`[FFmpeg] ${message}`));

    const baseURL = 'https://unpkg.com/@ffmpeg/core@0.12.6/dist/umd';

    // Menggunakan helper toBlobURL buatan sendiri
    await ffmpeg.load({
        coreURL: await toBlobURL(`${baseURL}/ffmpeg-core.js`, 'text/javascript'),
        wasmURL: await toBlobURL(`${baseURL}/ffmpeg-core.wasm`, 'application/wasm'),
        workerURL: await toBlobURL(`${baseURL}/ffmpeg-core.worker.js`, 'text/javascript')
    });

    ffmpegLoaded = true;
    logConsole("ffmpeg.wasm core engine loaded successfully.");
}

// Fetch Video Info
fetchBtn.addEventListener("click", async () => {
    const url = videoUrlInput.value.trim();
    let proxy = corsProxyInput.value.trim() || null;
    if (proxy && !proxy.endsWith("/")) proxy += "/";

    if (!url) { alert("Please enter a YouTube video URL."); return; }

    resultsCard.classList.add("hidden");
    progressCard.classList.remove("hidden");
    fetchBtn.disabled = true;
    fetchBtn.querySelector(".loader-spinner").classList.remove("hidden");

    logConsole("--- Starting Video Extraction ---");
    logConsole(`Target URL: ${url}`);
    logConsole(`CORS Proxy: ${proxy || "NONE (Direct fetch)"}`);

    try {
        currentVideoInfo = await wasm_extract_info(url, proxy);
        logConsole(`Successfully retrieved details for: "${currentVideoInfo.title}"`);

        // Populate metadata
        videoThumbnail.src = currentVideoInfo.thumbnail || "";
        videoTitle.textContent = currentVideoInfo.title;
        videoDuration.textContent = `Duration: ${formatDuration(currentVideoInfo.duration)}`;
        videoUploader.textContent = `Uploader: ${currentVideoInfo.channel || currentVideoInfo.uploader || "Unknown"}`;
        videoViews.textContent = `Views: ${currentVideoInfo.view_count ? currentVideoInfo.view_count.toLocaleString() : "unknown"}`;
        videoDesc.textContent = currentVideoInfo.description || "";

        // Populate Format Selectors
        videoFormatSelect.innerHTML = "";
        audioFormatSelect.innerHTML = "";

        const formats = currentVideoInfo.formats || [];
        logConsole(`Total formats received: ${formats.length}`);

        // Video formats: has vcodec set and not "none"
        const videoFormats = formats.filter(f =>
            f.vcodec && f.vcodec !== "none" && f.url
        ).sort((a, b) => (b.height || 0) - (a.height || 0));

        // Audio formats: no vcodec (or vcodec === "none") AND has acodec
        const audioFormats = formats.filter(f =>
            f.acodec && f.acodec !== "none" && (!f.vcodec || f.vcodec === "none") && f.url
        ).sort((a, b) => (b.abr || 0) - (a.abr || 0));

        // If no separate streams, show combined formats in both lists
        const combinedFormats = formats.filter(f =>
            f.vcodec && f.vcodec !== "none" &&
            f.acodec && f.acodec !== "none" &&
            f.url
        );

        // Fill video select
        if (videoFormats.length > 0) {
            videoFormats.forEach(f => {
                const size = f.filesize ? formatBytes(f.filesize) : (f.filesize_approx ? `~${formatBytes(f.filesize_approx)}` : "?");
                const res = f.height ? `${f.height}p` : "?";
                const fps = f.fps ? `@${Math.round(f.fps)}fps` : "";
                const combined = (f.acodec && f.acodec !== "none") ? " [+Audio]" : "";
                const opt = document.createElement("option");
                opt.value = f.format_id;
                opt.textContent = `${res}${fps} | ${f.ext} | ${f.vcodec}${combined} | ${size}`;
                videoFormatSelect.appendChild(opt);
            });
        } else if (combinedFormats.length > 0) {
            combinedFormats.forEach(f => {
                const size = f.filesize ? formatBytes(f.filesize) : "?";
                const res = f.height ? `${f.height}p` : "?";
                const opt = document.createElement("option");
                opt.value = f.format_id;
                opt.textContent = `${res} | ${f.ext} | combined (${size})`;
                videoFormatSelect.appendChild(opt);
            });
        }

        // Fill audio select
        if (audioFormats.length > 0) {
            audioFormats.forEach(f => {
                const size = f.filesize ? formatBytes(f.filesize) : (f.filesize_approx ? `~${formatBytes(f.filesize_approx)}` : "?");
                const abr = f.abr ? `${Math.round(f.abr)}kbps` : "?kbps";
                const opt = document.createElement("option");
                opt.value = f.format_id;
                opt.textContent = `${abr} | ${f.ext} | ${f.acodec} | ${size}`;
                audioFormatSelect.appendChild(opt);
            });
        } else if (combinedFormats.length > 0) {
            combinedFormats.forEach(f => {
                const size = f.filesize ? formatBytes(f.filesize) : "?";
                const opt = document.createElement("option");
                opt.value = f.format_id;
                opt.textContent = `combined audio | ${f.ext} | ${size}`;
                audioFormatSelect.appendChild(opt);
            });
        }

        logConsole(`Video formats: ${videoFormatSelect.options.length} | Audio formats: ${audioFormatSelect.options.length}`);
        resultsCard.classList.remove("hidden");
    } catch (e) {
        logConsole(`ERROR: Failed to extract info: ${e}`);
        alert("Failed to extract video details. Make sure your CORS proxy is active.");
    } finally {
        fetchBtn.disabled = false;
        fetchBtn.querySelector(".loader-spinner").classList.add("hidden");
    }
});

// Download Trigger
downloadBtn.addEventListener("click", async () => {
    if (!currentVideoInfo) return;

    const proxy = (() => { const v = corsProxyInput.value.trim(); return v ? (v.endsWith("/") ? v : v + "/") : null; })();
    const videoFormatId = videoFormatSelect.value;
    const audioFormatId = audioFormatSelect.value;
    const mode = downloadMode;

    // Validate selections
    const needsVideo = (mode === "merge" || mode === "video-only");
    const needsAudio = (mode === "merge" || mode === "mp3" || mode === "audio-only");

    if (needsVideo && !videoFormatId) { alert("Please select a video format."); return; }
    if (needsAudio && !audioFormatId) { alert("Please select an audio format."); return; }

    // Load FFmpeg for modes that need it
    if (mode === "merge" || mode === "mp3") {
        try {
            await ensureFFmpegLoaded();
        } catch (e) {
            logConsole(`ERROR: FFmpeg loading failed: ${e}`);
            alert("Could not load ffmpeg.wasm. Please check the console.");
            return;
        }
    }

    // Lock UI
    downloadBtn.disabled = true;
    downloadBtn.querySelector(".loader-spinner").classList.remove("hidden");
    progressCard.classList.remove("hidden");
    videoProgressRow.classList.add("hidden");
    audioProgressRow.classList.add("hidden");
    mergeProgressRow.classList.add("hidden");

    // Re-fetch fresh stream URLs
    logConsole("Re-fetching fresh stream URLs before download...");
    let freshFormats = currentVideoInfo.formats;
    try {
        const freshInfo = await wasm_extract_info(currentVideoInfo.webpage_url || videoUrlInput.value.trim(), proxy);
        freshFormats = freshInfo.formats;
        logConsole("Fresh URLs obtained successfully.");
    } catch (e) {
        logConsole(`Warning: Could not re-fetch (${e}). Using cached URLs.`);
    }

    const findFormat = id => freshFormats.find(f => f.format_id === id) ||
        currentVideoInfo.formats.find(f => f.format_id === id);
    const freshVideo = needsVideo ? findFormat(videoFormatId) : null;
    const freshAudio = needsAudio ? findFormat(audioFormatId) : null;

    // Helper: download a format
    async function downloadBytes(fmt, progressBar, progressPct, progressRow, label) {
        progressRow.classList.remove("hidden");
        progressBar.style.width = "0%";
        progressPct.textContent = "0%";
        logConsole(`Downloading ${label}: format ${fmt.format_id} (${fmt.protocol})`);
        const bytes = await wasm_download_format(
            fmt.url,
            fmt.protocol,
            JSON.stringify(fmt.http_headers || {}),
            proxy,
            (pos, total) => {
                const pct = total > 0 ? ((pos / total) * 100).toFixed(1) : "...";
                progressBar.style.width = total > 0 ? `${pct}%` : "50%";
                progressPct.textContent = total > 0 ? `${pct}%` : `${pos} segs`;
            }
        );
        progressBar.style.width = "100%";
        progressPct.textContent = "100%";
        logConsole(`${label} download complete (${formatBytes(bytes.length)}).`);
        return bytes;
    }

    const title = sanitizeFilename(currentVideoInfo.title);

    try {
        // ── MODE: Video + Audio merged to MKV ──────────────────────────────────
        if (mode === "merge") {
            const vBytes = await downloadBytes(freshVideo, videoProgressBar, videoProgressPercent, videoProgressRow, "Video");
            const aBytes = await downloadBytes(freshAudio, audioProgressBar, audioProgressPercent, audioProgressRow, "Audio");

            mergeProgressRow.classList.remove("hidden");
            mergeProgressStatus.textContent = "Muxing streams with ffmpeg.wasm...";
            logConsole("Running FFmpeg mux...");

            const vExt = freshVideo.ext || "mp4";
            const aExt = freshAudio.ext || "m4a";
            const outName = "output.mkv";

            await ffmpeg.writeFile(`inv.${vExt}`, vBytes);
            await ffmpeg.writeFile(`ina.${aExt}`, aBytes);
            await ffmpeg.exec(["-i", `inv.${vExt}`, "-i", `ina.${aExt}`, "-c", "copy", outName]);

            const outData = await ffmpeg.readFile(outName);
            triggerBlobDownload(outData, `${title}.mkv`, "video/x-matroska");
            logConsole("✅ MKV download complete!");

            await ffmpeg.deleteFile(`inv.${vExt}`).catch(() => { });
            await ffmpeg.deleteFile(`ina.${aExt}`).catch(() => { });
            await ffmpeg.deleteFile(outName).catch(() => { });
        }

        // ── MODE: Audio → MP3 conversion ───────────────────────────────────────
        else if (mode === "mp3") {
            const aBytes = await downloadBytes(freshAudio, audioProgressBar, audioProgressPercent, audioProgressRow, "Audio");

            mergeProgressRow.classList.remove("hidden");
            mergeProgressStatus.textContent = "Converting to MP3 with ffmpeg.wasm...";
            logConsole("Running FFmpeg MP3 conversion (libmp3lame)...");

            const aExt = freshAudio.ext || "m4a";
            const inName = `audio_in.${aExt}`;
            const outName = "output.mp3";

            await ffmpeg.writeFile(inName, aBytes);
            await ffmpeg.exec([
                "-i", inName,
                "-vn",                  // no video
                "-acodec", "libmp3lame",
                "-q:a", "2",            // VBR ~190kbps
                "-ar", "44100",
                outName
            ]);

            const outData = await ffmpeg.readFile(outName);
            triggerBlobDownload(outData, `${title}.mp3`, "audio/mpeg");
            logConsole("✅ MP3 download complete!");

            await ffmpeg.deleteFile(inName).catch(() => { });
            await ffmpeg.deleteFile(outName).catch(() => { });
        }

        // ── MODE: Video Only ───────────────────────────────────────────────────
        else if (mode === "video-only") {
            const vBytes = await downloadBytes(freshVideo, videoProgressBar, videoProgressPercent, videoProgressRow, "Video");
            triggerBlobDownload(vBytes, `${title}.${freshVideo.ext || "mp4"}`, "video/mp4");
            logConsole("✅ Video download complete!");
        }

        // ── MODE: Audio Only (raw) ─────────────────────────────────────────────
        else if (mode === "audio-only") {
            const aBytes = await downloadBytes(freshAudio, audioProgressBar, audioProgressPercent, audioProgressRow, "Audio");
            triggerBlobDownload(aBytes, `${title}.${freshAudio.ext || "m4a"}`, "audio/mp4");
            logConsole("✅ Audio download complete!");
        }

    } catch (e) {
        logConsole(`ERROR: ${e}`);
        alert(`Download failed: ${e}`);
    } finally {
        downloadBtn.disabled = false;
        downloadBtn.querySelector(".loader-spinner").classList.add("hidden");
        mergeProgressRow.classList.add("hidden");
    }
});

function triggerBlobDownload(bytes, filename, mimeType = "application/octet-stream") {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes.buffer || bytes);
    const blob = new Blob([data], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}

// Start
start();
