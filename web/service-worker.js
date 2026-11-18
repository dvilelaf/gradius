// Offline shell for the wasm page. Bump VERSION whenever a cached file changes:
// activate drops every cache whose name is not the current one.
"use strict";

const VERSION = "gradius-pwa-v4";
const CACHE = `gradius-${VERSION}`;
// Everything the game needs after the first visit. `layout.mjs` is imported by
// `app.js`. The extractor wasm is cached on first use; the pack and the ROM
// are never cached here (the pack lives in IndexedDB).
const PRECACHE = [
  "./",
  "./index.html",
  "./app.js",
  "./layout.mjs",
  "./audio-worklet.js",
  "./manifest.webmanifest",
  "./gradius_wasm.wasm",
  "./icon.svg",
  "./extract.mjs",
  "./extract-worker.js",
  "./pack-store.mjs",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name !== CACHE)
            .map((name) => caches.delete(name)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

// Cache-first: the precache answers immediately, offline or not. A miss is
// fetched and stored so the manifest and icons work without a second visit.
// Navigation falls back to the cached page when the network is gone.
self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (/\.(pak|nes)$/.test(url.pathname)) return;

  event.respondWith(
    caches.match(request).then(
      (cached) =>
        cached ||
        fetch(request)
          .then((response) => {
            if (response.ok) {
              const copy = response.clone();
              caches.open(CACHE).then((cache) => cache.put(request, copy));
            }
            return response;
          })
          .catch(() =>
            request.mode === "navigate"
              ? caches.match("./index.html")
              : Response.error(),
          ),
    ),
  );
});
