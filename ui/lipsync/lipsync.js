/* "Липсинк" tab (LatentSync 1.6): photo or video + audio -> lip-synced video. Loaded lazily by index.html (loadTab).
   Same plumbing and styles as the video tab (ui/ltx/video.js, video.css). Media is converted by the server with ffmpeg first. */
(() => {
const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const j = async (url, opt) => { const r = await fetch(url, opt); const d = await r.json().catch(() => ({})); if (!r.ok) throw Object.assign(new Error(d.error?.message || d.error || r.statusText), {status: r.status, body: d}); return d; };
const post = (url, body) => j(url, {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify(body)});
const upload = (kind, file) => j('/api/lipsync/prepare?kind=' + kind, {method:'POST', body: file});

// measured on an RTX 4080 SUPER 16 GB: seconds of work per second of result (the model is memory-bound there); +60 s fixed for loading
const COST = {q: 120, f: 45}, MAX_SEC = 60;
const QUALITY = {q: {lips: 1.5, steps: 12}, f: {lips: 1.0, steps: 12}};
const L = {tpl: {}, jobs: [], shown: null, timer: null, pct: 0, audioSec: 0, videoSec: 0, missing: 0};
const get = (k, d) => { const v = store.get('ls_' + k); return v === null ? d : v; };

function build(root) {
  root.innerHTML = `
  <form class="card composer" id="lForm" autocomplete="off">
    <div class="seg" id="lModeSeg"><span class="thumb"></span>
      <button type="button" data-value="v">Видео + аудио</button><button type="button" data-value="p">Фото + аудио</button></div>
    <input type="hidden" id="lMode" value="v">
    <div class="v-tip" id="lModeNote"></div>

    <div class="drop" id="lDrop" tabindex="0" role="button" aria-label="Выбрать файл">
      <div class="hint"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="1.6"/><path d="M21 16l-5-5-8 8"/></svg><span id="lDropText"></span></div>
      <img alt=""><video muted playsinline style="position:absolute;inset:0;width:100%;height:100%;object-fit:cover;display:none"></video>
      <button class="x" type="button" aria-label="Убрать файл">×</button>
      <input id="lMedia" type="file">
    </div>

    <label class="l">Аудио с речью (wav, mp3, m4a…)
      <input class="field" id="lAudio" type="file" accept="audio/*,video/*" style="border-radius:20px">
    </label>
    <audio id="lAudioPrev" controls hidden style="width:100%;height:36px"></audio>

    <div class="seg small" id="lQSeg"><span class="thumb"></span>
      <button type="button" data-value="q">Качество</button><button type="button" data-value="f">Быстро</button></div>
    <input type="hidden" id="lQ" value="q">
    <div class="v-tip" id="lQNote"></div>

    <label class="l" id="lLenRow">Если видео и аудио разной длины
      <select id="lLen"><option value="normal">обрезать по более короткому</option><option value="pingpong">зациклить видео под аудио</option></select>
    </label>

    <div class="meta-col">
      <div class="meta-sub"><span>Seed</span><label class="switch mini"><input id="lRand" type="checkbox" checked> авто</label></div>
      <input id="lSeed" type="number" min="0" value="1247" aria-label="Seed" disabled>
    </div>

    <div class="v-box" id="lHint"></div>
    <div class="go-wrap" id="lGoWrap">
      <button class="go" id="lGo" type="submit"><span class="lbl"><svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg><span id="lGoText">Синхронизировать губы</span></span></button>
      <button class="cancel-btn" id="lCancel" type="button"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="5" y="5" width="14" height="14" rx="2.5"/></svg><span>Отменить</span></button>
    </div>
    <div id="lStatus" role="status" style="text-align:center;font-size:13px;color:var(--mute);min-height:18px"></div>
  </form>

  <div class="right"><section class="card results">
    <div class="v-stage" id="lStage">
      <div class="v-empty" id="lEmpty">Здесь появится видео<small>Файлы сохраняются в E:\\ComfyUI\\ComfyUI\\output\\lipsync</small></div>
      <video id="lPlayer" controls loop playsinline hidden></video>
      <div class="v-prog" id="lProg" hidden>
        <div class="t"><span id="lProgLbl"></span><span id="lProgTime"></span></div>
        <div class="bar-wrap"><div class="bar-fill" id="lProgBar"></div></div>
      </div>
    </div>
    <div class="v-box bad" id="lErr" hidden></div>
    <div class="v-actions" id="lActions" hidden>
      <a id="lSave" download>Сохранить</a>
      <button type="button" id="lFolder">Открыть папку</button>
      <button type="button" id="lReuse">Те же настройки</button>
      <button type="button" id="lRegen">Ещё раз, новый seed</button>
      <button type="button" class="del" id="lDel">Удалить</button>
    </div>
    <div class="v-meta" id="lMeta"></div>
    <div class="v-hist-h"><h3>История</h3><span id="lHistN"></span></div>
    <div id="lHist"></div>
  </section></div>`;
}

const photo = () => $('lMode').value === 'p';
const say = (t, err) => { $('lStatus').textContent = t; $('lStatus').style.color = err ? 'var(--red)' : ''; };
const showErr = html => { $('lErr').hidden = !html; $('lErr').innerHTML = html || ''; };
const urlOf = x => `/view?filename=${encodeURIComponent(x.filename)}&subfolder=${encodeURIComponent(x.subfolder || '')}&type=output`;
const fmtMin = s => s < 90 ? `${Math.round(s)} с` : `${(s / 60).toFixed(1)} мин`;

function resultSeconds() {
  const a = L.audioSec, v = L.videoSec;
  if (photo() || !v) return a;
  return $('lLen').value === 'normal' ? Math.min(a, v) : Math.max(a, v);
}
function refresh() {
  $('lModeNote').textContent = photo() ? 'Фото держится кадром всю речь, двигаются только губы. Лицо должно быть видно анфас.' : 'Остальной кадр остаётся вашим, меняется только область рта. Лицо должно быть видно в каждом кадре.';
  $('lDropText').textContent = photo() ? 'Фото с лицом' : 'Видео с лицом (mp4, mov, webm…)';
  $('lMedia').accept = photo() ? 'image/*' : 'video/*';
  $('lLenRow').hidden = photo();
  $('lQNote').textContent = $('lQ').value === 'q' ? 'Полное управление губами: точнее синхронизация, дольше в 2-3 раза.' : 'Без усиления губ: быстрее, синхронизация чуть слабее.';
  const sec = resultSeconds(), box = $('lHint');
  if (!L.audioSec) { box.className = 'v-box'; box.textContent = 'Выберите аудио, и я покажу, сколько это займёт.'; }
  else if (sec > MAX_SEC) { box.className = 'v-box bad'; box.innerHTML = `<b>Слишком длинно.</b> Результат ${sec.toFixed(0)} с, максимум ${MAX_SEC} с. Обрежьте аудио.`; }
  else {
    const eta = 60 + sec * COST[$('lQ').value];
    box.className = 'v-box ' + (eta > 1800 ? 'warn' : 'ok');
    box.innerHTML = `Результат ${sec.toFixed(1)} с, это займёт около <b>${fmtMin(eta)}</b> (видеокарта будет загружена полностью).`;
  }
  store.set('ls_form', JSON.stringify({mode: $('lMode').value, q: $('lQ').value, len: $('lLen').value}));
}

/* ---------- job flow ---------- */
function tpl(mode) { return L.tpl[mode] || (L.tpl[mode] = j(`/lipsync/lipsync_${mode}_api.json`)); }
function fillTpl(n, v) {
  if (Array.isArray(n)) return n.map(x => fillTpl(x, v));
  if (n && typeof n === 'object') return Object.fromEntries(Object.entries(n).map(([k, x]) => [k, fillTpl(x, v)]));
  const m = typeof n === 'string' && n.match(/^\{\{(\w+)\}\}$/);
  return m ? v[m[1]] : n;
}
async function submit(s) {
  showErr('');
  $('lGo').disabled = true;
  try {
    if (!s.mediaName) {
      const f = $('lMedia').files[0];
      if (!f) throw new Error(photo() ? 'Выберите фото' : 'Выберите видео');
      say('Готовлю ' + (s.mode === 'p' ? 'фото' : 'видео') + '…');
      s.mediaName = s.mode === 'p' ? await uploadPhoto(f) : (await upload('video', f)).name;
    }
    if (!s.audioName) {
      const f = $('lAudio').files[0];
      if (!f) throw new Error('Выберите аудио');
      say('Готовлю аудио…');
      const a = await upload('audio', f); s.audioName = a.name; s.audioSec = a.seconds;
    }
    if (s.audioSec > MAX_SEC + 1) throw new Error(`Аудио длиннее ${MAX_SEC} с`);
    s.frames = Math.ceil(s.audioSec * 25);
    say(L.jobs.length ? 'Добавляю в очередь…' : 'Запускаю ComfyUI и отправляю задачу… (первый запуск до минуты)');
    const q = QUALITY[s.q], prog = await j('/api/ltx/progress').catch(() => ({})), logPos = (await j('/api/lipsync/progress').catch(() => ({}))).size || 0;
    const g = fillTpl(await tpl(s.mode === 'p' ? 'p2v' : 'v2v'), {video: s.mediaName, image: s.mediaName, audio: s.audioName, frames: s.frames, mode: s.len, seed: s.seed, lips: q.lips, steps: q.steps});
    const r = await post('/prompt', {prompt: g, client_id: prog.cid});
    L.jobs.push({pid: r.prompt_id, s, logPos}); say('');
    startLoop();
  } catch (e) {
    const b = e.body;
    if (e.status === 502) say('ComfyUI не запущен и не смог стартовать. Смотри logs\\comfyui.out.log', true);
    else if (b?.node_errors && Object.keys(b.node_errors).length) showErr(`<b>ComfyUI отклонил задачу.</b><br>${esc(Object.values(b.node_errors)[0].errors?.[0]?.message || '')}<br>Проверьте, что ComfyUI-LatentSyncWrapper и его модели на месте.`);
    else say(String(e.message).includes('Failed to fetch') ? 'Нет связи с сервером Студии' : e.message, true);
  } finally { $('lGo').disabled = false; }
}
function startLoop() { syncBusy(); if (!L.timer) L.timer = setInterval(tick, 1000); }
function syncBusy() {
  const busy = L.jobs.length > 0;
  $('lGoWrap').classList.toggle('busy', busy);
  $('lGoText').textContent = busy ? 'В очередь' : 'Синхронизировать губы';
  $('lProg').hidden = !busy;
  if (busy) $('lPlayer').pause();
}
let ticking = false;
async function tick() {
  if (ticking || !L.jobs.length) return;
  ticking = true;
  const job = L.jobs[0];
  try {
    const [p, h, q] = await Promise.all([j('/api/lipsync/progress?since=' + (job.logPos || 0)), j('/history/' + job.pid).catch(() => ({})), j('/queue').catch(() => ({}))]);
    const item = h[job.pid];
    if (item?.status?.completed || item?.status?.status_str === 'error') return await finish(job, item);
    const running = (q.queue_running || []).some(x => x[1] === job.pid), queued = (q.queue_pending || []).some(x => x[1] === job.pid);
    if (running && !job.t0) job.t0 = Date.now();
    if (!running && !queued && !item) { if (++L.missing > 20) { L.missing = 0; L.jobs.shift(); showErr('<b>ComfyUI был перезапущен, задача потеряна.</b> Запустите её ещё раз.'); syncBusy(); } return; }
    L.missing = 0;
    let label = job.t0 ? 'Подготовка (поиск лица)…' : 'Ждёт очереди…', pct = L.pct;
    if (job.t0 && p.total > 0) { pct = Math.max(pct, 5 + 90 * p.step / p.total); label = `Синхронизация губ · часть ${Math.min(p.step + 1, p.total)} из ${p.total}`; if (p.step >= p.total) label = 'Сборка видео…'; }
    L.pct = pct;
    $('lProgBar').style.width = pct + '%';
    $('lProgLbl').textContent = `${label} · ${Math.round(pct)}%`;
    $('lProgTime').textContent = job.t0 ? fmtMin((Date.now() - job.t0) / 1000) : '';
  } catch { /* server hiccup: try again on the next tick */ } finally { ticking = false; }
}
function errText(type, msg) {
  const m = `${type} ${msg}`.toLowerCase();
  if (m.includes('out of memory') || m.includes('outofmemory')) return '<b>Не хватило видеопамяти.</b><ul><li>выберите режим «Быстро»</li><li>возьмите видео меньшего разрешения</li><li>закройте игры и другие программы на видеокарте</li></ul>';
  if (m.includes('face') || m.includes('лиц')) return '<b>Лицо не найдено.</b> Нужно чёткое лицо, видное в каждом кадре.';
  return `<b>ComfyUI вернул ошибку.</b><br>${esc(String(msg).split('\n')[0])}`;
}
async function finish(job, item) {
  L.jobs.shift(); L.pct = 0; $('lProgBar').style.width = '0%';
  const st = item.status?.status_str;
  if (st === 'success') {
    const out = Object.values(item.outputs || {}).flatMap(o => o.images || []).find(i => /\.mp4$/i.test(i.filename));
    if (out) {
      const meta = {...job.s, elapsed: job.t0 ? Math.round((Date.now() - job.t0) / 1000) : null, created: new Date().toISOString()};
      await post('/api/ltx/meta', {filename: out.filename, meta}).catch(() => {});
      await loadHistory();
      show({filename: out.filename, subfolder: out.subfolder || 'lipsync', meta});
    } else showErr('<b>ComfyUI закончил, но видео не найдено.</b>');
  } else {
    const e = (item.status?.messages || []).find(m => m[0] === 'execution_error')?.[1] || {};
    showErr(errText(e.exception_type || '', e.exception_message || 'неизвестная ошибка'));
  }
  if (!L.jobs.length) { clearInterval(L.timer); L.timer = null; }
  syncBusy();
}
async function cancel() {
  if (!L.jobs.length) return;
  L.jobs.length = 0; say('Останавливаю…');
  await post('/interrupt', {}).catch(() => {});
  await post('/api/lipsync/cancel', {}).catch(() => {});   // the model cannot be interrupted mid-run, so ComfyUI is restarted
  L.pct = 0; clearInterval(L.timer); L.timer = null; syncBusy(); say('Остановлено');
}

/* ---------- result + history ---------- */
function show(x) {
  L.shown = x;
  const v = $('lPlayer'); v.src = urlOf(x); v.hidden = false; $('lEmpty').hidden = true; v.play().catch(() => {});
  $('lActions').hidden = false; $('lSave').href = urlOf(x); $('lSave').download = x.filename;
  const m = x.meta || {};
  $('lMeta').innerHTML = m.mode ? `${m.mode === 'p' ? 'Фото + аудио' : 'Видео + аудио'} · ${m.audioSec ? m.audioSec.toFixed(1) + ' с · ' : ''}${m.q === 'f' ? 'быстро' : 'качество'} · seed ${m.seed}${m.elapsed ? ` · ${fmtMin(m.elapsed)}` : ''}` : esc(x.filename);
  document.querySelectorAll('#lHist .v-card').forEach(c => c.classList.toggle('on', c.dataset.f === x.filename));
}
async function loadHistory() {
  let list = []; try { list = await j('/api/lipsync/history'); } catch {}
  $('lHistN').textContent = list.length || '';
  $('lHist').innerHTML = list.length ? '' : '<div class="v-tip">Пока пусто</div>';
  list.forEach(x => {
    const m = x.meta || {}, c = document.createElement('div');
    c.className = 'v-card'; c.dataset.f = x.filename;
    c.innerHTML = `<video muted preload="metadata" src="${urlOf(x)}#t=0.1"></video><div class="cap">${m.mode ? (m.mode === 'p' ? 'Фото + аудио' : 'Видео + аудио') : esc(x.filename)}<br><i>${m.audioSec ? m.audioSec.toFixed(1) + ' с' : ''}</i></div>`;
    c.onclick = () => show(x);
    $('lHist').appendChild(c);
  });
}
function applySettings(m) {
  $('lModeSeg').querySelector(`[data-value="${m.mode || 'v'}"]`).click();
  $('lQSeg').querySelector(`[data-value="${m.q || 'q'}"]`).click();
  $('lLen').value = m.len || 'normal'; $('lSeed').value = m.seed; $('lRand').checked = false; $('lSeed').disabled = false;
  refresh();
}

/* ---------- wiring ---------- */
function wire() {
  seg($('lModeSeg'), $('lMode'), () => { window.clearMedia?.(); refresh(); });
  seg($('lQSeg'), $('lQ'), refresh);
  $('lLen').onchange = refresh;
  $('lRand').onchange = () => { $('lSeed').disabled = $('lRand').checked; };

  const drop = $('lDrop'), input = $('lMedia'), img = drop.querySelector('img'), vid = drop.querySelector('video');
  const pick = f => { const dt = new DataTransfer(); if (f) dt.items.add(f); input.files = dt.files; input.dispatchEvent(new Event('change')); };
  drop.onclick = e => { if (!e.target.closest('.x')) input.click(); };
  drop.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); input.click(); } };
  drop.querySelector('.x').onclick = () => pick(null);
  ['dragenter', 'dragover'].forEach(t => drop.addEventListener(t, e => { e.preventDefault(); drop.classList.add('over'); }));
  ['dragleave', 'drop'].forEach(t => drop.addEventListener(t, e => { e.preventDefault(); drop.classList.remove('over'); }));
  drop.addEventListener('drop', e => { const f = e.dataTransfer.files[0]; if (f?.type.startsWith(photo() ? 'image/' : 'video/')) pick(f); });
  input.onchange = () => {
    const f = input.files[0]; L.videoSec = 0;
    img.removeAttribute('src'); vid.removeAttribute('src'); vid.style.display = 'none'; img.style.display = '';
    if (f && photo()) img.src = URL.createObjectURL(f);
    if (f && !photo()) { img.style.display = 'none'; vid.style.display = ''; vid.src = URL.createObjectURL(f); vid.onloadedmetadata = () => { L.videoSec = vid.duration; refresh(); }; }
    drop.classList.toggle('has', !!f); refresh();
  };
  window.clearMedia = () => pick(null);

  $('lAudio').onchange = () => {
    const f = $('lAudio').files[0], a = $('lAudioPrev'); L.audioSec = 0;
    a.hidden = !f; if (!f) { refresh(); return; }
    a.src = URL.createObjectURL(f); a.onloadedmetadata = () => { L.audioSec = a.duration; refresh(); };
    a.onerror = () => { say('Браузер не показал аудио, но сервер попробует его прочитать', false); refresh(); };
  };

  $('lForm').onsubmit = e => {
    e.preventDefault();
    if ($('lRand').checked) $('lSeed').value = Math.floor(Math.random() * 2 ** 31);
    submit({mode: $('lMode').value, q: $('lQ').value, len: $('lLen').value, seed: +$('lSeed').value, audioSec: L.audioSec});
  };
  $('lCancel').onclick = cancel;
  $('lFolder').onclick = () => post('/api/ltx/open-folder', {filename: L.shown.filename}).catch(e => say(e.message, true));
  $('lReuse').onclick = () => L.shown.meta?.mode ? applySettings(L.shown.meta) : say('У этого файла нет сохранённых настроек', true);
  $('lRegen').onclick = () => {
    const m = L.shown.meta;
    if (!m?.mediaName) return say('У этого файла нет сохранённых настроек', true);
    submit({...m, seed: Math.floor(Math.random() * 2 ** 31), elapsed: undefined, created: undefined});   // same prepared inputs are still in ComfyUI's input folder
  };
  $('lDel').onclick = async () => {
    if (!confirm(`Удалить ${L.shown.filename} с диска? Это нельзя отменить.`)) return;
    await post('/api/ltx/delete', {filename: L.shown.filename}).catch(e => say(e.message, true));
    $('lPlayer').removeAttribute('src'); $('lPlayer').hidden = true; $('lEmpty').hidden = false; $('lActions').hidden = true; $('lMeta').textContent = '';
    L.shown = null; loadHistory();
  };
}

window.initLipsyncTab = async function (root) {
  const saved = (() => { try { return JSON.parse(get('form', '{}')); } catch { return {}; } })();   // read before wire(): wiring saves defaults
  build(root); wire();
  $('lModeSeg').querySelector(`[data-value="${saved.mode || 'v'}"]`).click();
  $('lQSeg').querySelector(`[data-value="${saved.q || 'q'}"]`).click();
  $('lLen').value = saved.len || 'normal';
  refresh(); loadHistory();
  try {   // a job may still be running from before the page was reloaded
    const q = await j('/queue');
    for (const it of [...(q.queue_running || []), ...(q.queue_pending || [])]) if (it[2]?.['7']?.inputs?.filename_prefix === 'lipsync/ls') L.jobs.push({pid: it[1], s: {}});
    if (L.jobs.length) startLoop();
  } catch {}
};
})();
