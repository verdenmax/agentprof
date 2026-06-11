// M2.3 dashboard polling layer (T6 stub; T11 fleshes out toolbar).
(function() {
    'use strict';
    const main = document.getElementById('main');
    if (!main) { console.warn('agentprof: no #main element; polling disabled'); return; }
    const intervalSec = parseInt(
        localStorage.getItem('agentprof.interval')
        || document.body.dataset.intervalDefault
        || '5',
        10
    );
    if (intervalSec <= 0) return;
    setInterval(async () => {
        try {
            const url = new URL(
                '/api' + window.location.pathname + '.html',
                window.location.origin
            );
            url.search = window.location.search;
            const r = await fetch(url, { headers: { 'Accept': 'text/html' } });
            if (r.ok) main.innerHTML = await r.text();
        } catch (e) {
            console.warn('agentprof poll error:', e);
        }
    }, intervalSec * 1000);
})();
