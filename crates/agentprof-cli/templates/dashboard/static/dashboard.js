// M2.3 dashboard polling + toolbar (full implementation).
(function() {
    'use strict';
    const main = document.getElementById('main');
    const status = document.getElementById('refresh-status');
    const pauseBtn = document.getElementById('btn-pause');
    const intervalSel = document.getElementById('sel-interval');
    if (!main || !status || !pauseBtn || !intervalSel) {
        console.warn('agentprof: missing UI elements; poller disabled');
        return;
    }

    let intervalSec = parseInt(
        localStorage.getItem('agentprof.interval')
        || document.body.dataset.intervalDefault
        || '5',
        10
    );
    let paused = localStorage.getItem('agentprof.paused') === '1';
    let timer = null;

    function setIntervalSelValue() { intervalSel.value = String(intervalSec); }
    function setPauseBtnText() { pauseBtn.textContent = paused ? '▶ Resume' : '⏸ Pause'; }

    async function refresh() {
        if (paused) return;
        const apiPath = '/api' + window.location.pathname + '.html';
        try {
            const r = await fetch(apiPath + window.location.search, { headers: { 'Accept': 'text/html' } });
            if (r.ok) {
                main.innerHTML = await r.text();
                status.textContent = 'updated ' + new Date().toLocaleTimeString();
            } else {
                status.textContent = 'refresh failed: ' + r.status;
            }
        } catch (e) {
            status.textContent = 'refresh error: ' + e.message;
        }
    }

    function applyTimer() {
        if (timer) { clearInterval(timer); timer = null; }
        if (!paused && intervalSec > 0) {
            timer = setInterval(refresh, intervalSec * 1000);
        }
    }

    pauseBtn.addEventListener('click', () => {
        paused = !paused;
        localStorage.setItem('agentprof.paused', paused ? '1' : '0');
        setPauseBtnText();
        applyTimer();
        if (!paused) refresh();
    });
    intervalSel.addEventListener('change', (e) => {
        intervalSec = parseInt(e.target.value, 10);
        localStorage.setItem('agentprof.interval', String(intervalSec));
        applyTimer();
    });

    setIntervalSelValue();
    setPauseBtnText();
    applyTimer();
})();
