/* global self, caches */
/* Remove API responses cached by earlier releases, including private data. */
self.addEventListener("activate", (event) => {
  event.waitUntil(caches.delete("api-cache"));
});
