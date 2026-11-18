// The extracted pack lives in this browser's IndexedDB; it is personal data
// and never leaves the device.
const DB = "gradius";
const STORE = "pack";
const KEY = "gradius.pak";

function open() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function run(mode, action) {
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const request = action(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(request.result);
      tx.onerror = tx.onabort = () => reject(tx.error);
    });
  } finally {
    db.close();
  }
}

/** @returns {Promise<Uint8Array | null>} */
export async function loadPack() {
  const pack = await run("readonly", (store) => store.get(KEY));
  return pack ?? null;
}

export const savePack = (pack) => run("readwrite", (store) => store.put(pack, KEY));
export const deletePack = () => run("readwrite", (store) => store.delete(KEY));
