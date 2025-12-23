// SPDX-License-Identifier: AGPL-3.0-only
async function sha256(message) {
    const msgBuffer = new TextEncoder().encode(message);                    
    const hashBuffer = await crypto.subtle.digest('SHA-256', msgBuffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    return hashHex;
}

async function generate(e) {
    if (e) {
        e.preventDefault();
    }
    let key = self.crypto.randomUUID();
    let hash = await sha256(key);
    console.log(key);
    console.log(hash);
    for (const elem of document.getElementsByClassName("ex-key")) {
        elem.innerHTML = key;
    }

    for (const elem of document.getElementsByClassName("ex-hash")) {
        elem.innerHTML = hash;
    }
}

document.getElementById('generate').addEventListener('click', generate);

generate();