/* "Видео (LTX-2.5)" tab. Loaded lazily by index.html on first open (see loadVideoTab).
   Talks to ComfyUI through the UI server: POST /prompt, GET /history, /api/ltx/progress (the server listens to ComfyUI's websocket), /view.
   Reuses globals of the main page: store, seg, fold, uploadPhoto, translate, CYR. */
(() => {
const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const j = async (url, opt) => { const r = await fetch(url, opt); const d = await r.json().catch(() => ({})); if (!r.ok) throw Object.assign(new Error(d.error?.message || d.error || r.statusText), {status: r.status, body: d}); return d; };
const post = (url, body) => j(url, {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify(body)});

const ASPECTS = {'16:9': [[768,448],[1024,576],[1280,704]], '9:16': [[448,768],[576,1024],[704,1280]], '1:1': [[512,512],[768,768],[960,960]]};
const FPS = [24, 25, 30];
const NEG = 'pc game, console game, video game, cartoon, childish, ugly';
const TIP = 'Кто/что в кадре, что делает, как движется камера, какой свет. Пример: «Рыжая лиса идёт по заснеженному лесу на рассвете, камера плавно едет следом, мягкий золотой свет, кинематографично».';
// ComfyUI node id -> [label, start %, share %]; ids come from build_templates.py
const PHASE = {'2':['Загрузка текстового энкодера',0,5], '6':['Кодирование промпта',5,3], '7':['Кодирование промпта',5,3], '8':['Кодирование промпта',5,3],
  '1':['Загрузка видео-модели',8,7], '3':['Загрузка VAE',8,2], '4':['Загрузка VAE',8,2], '5':['Загрузка VAE',8,2],
  '20':['Подготовка фото',8,2], '21':['Подготовка фото',8,2], '22':['Подготовка фото',8,2], '23':['Подготовка фото',8,2],
  '16':['Этап 1: черновик',15,35], '17':['Этап 1: черновик',50,0], '18':['Увеличение ×2',50,5], '24':['Увеличение ×2',50,5], '19':['Увеличение ×2',55,0],
  '33':['Этап 2: детали',55,20], '34':['Этап 2: детали',75,0], '35':['Декодирование видео',75,18], '36':['Декодирование звука',93,2], '37':['Сохранение',95,5], '38':['Сохранение',95,5]};

const V = {cfg:{}, files:null, jobs:[], shown:null, tpl:{}, mode:'t2v', timer:null, pct:0, loaded:false};
const get = (k, d) => { const v = store.get('ltx_' + k); return v === null ? d : v; };
const put = (k, v) => store.set('ltx_' + k, v);

/* ---------- markup ---------- */
function build(root) {
  root.innerHTML = `
  <form class="card composer" id="vForm" autocomplete="off">
    <div class="seg" id="vModeSeg"><span class="thumb"></span>
      <button type="button" data-value="t2v">Текст → видео</button>
      <button type="button" data-value="i2v">Фото → видео</button></div>
    <input type="hidden" id="vMode" value="t2v">

    <div class="fold" id="vFoldImg"><div class="in">
      <div class="drop" id="vDrop" tabindex="0" role="button" aria-label="Выбрать стартовое фото">
        <div class="hint"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="1.6"/><path d="M21 16l-5-5-8 8"/></svg><span>Стартовое фото (первый кадр)</span></div>
        <img alt=""><button class="x" type="button" aria-label="Убрать фото">×</button>
        <input id="vPhoto" type="file" accept="image/*">
      </div>
    </div></div>

    <textarea id="vPrompt" aria-label="Промпт" placeholder="${esc(TIP)}"></textarea>
    <label class="switch" title="Русский текст переводится на английский локально"><input id="vTr" type="checkbox" checked> переводить с русского на английский</label>
    <div class="v-tip" id="vTrOut" hidden></div>
    <textarea id="vNeg" class="short" aria-label="Негативный промпт" placeholder="Негативный промпт (пусто = стандартный)"></textarea>

    <div class="v-row2">
      <label class="l">Видео-модель (GGUF)<select id="vUnet"></select></label>
      <label class="l">Текстовый энкодер<select id="vClip"></select></label>
    </div>

    <div class="seg small" id="vAspSeg"><span class="thumb"></span>
      <button type="button" data-value="16:9">16:9</button><button type="button" data-value="9:16">9:16</button><button type="button" data-value="1:1">1:1</button></div>
    <input type="hidden" id="vAsp" value="16:9">
    <div class="seg small" id="vSizeSeg"><span class="thumb"></span>
      <button type="button" data-value="0">Малый</button><button type="button" data-value="1">Средний</button><button type="button" data-value="2">Большой</button></div>
    <input type="hidden" id="vSize" value="1">
    <div class="v-row2">
      <label class="l">Ширина<input id="vW" type="number" step="64" min="256" max="1920" inputmode="numeric"></label>
      <label class="l">Высота<input id="vH" type="number" step="64" min="256" max="1920" inputmode="numeric"></label>
    </div>
    <div class="v-tip" id="vSizeNote"></div>

    <div class="v-row2">
      <div class="meta-col"><div class="v-lbl"><span>Длительность</span><b id="vSecVal"></b></div><input id="vSec" type="range" min="1" max="10" step="0.5" aria-label="Длительность, секунд"></div>
      <label class="l">Кадров в секунду<select id="vFps">${FPS.map(f => `<option>${f}</option>`).join('')}</select></label>
    </div>
    <div class="v-tip" id="vFrames"></div>

    <div class="meta-col">
      <div class="meta-sub"><span>Seed</span><label class="switch mini"><input id="vRand" type="checkbox" checked> авто</label></div>
      <input id="vSeed" type="number" min="0" value="1" aria-label="Seed">
    </div>

    <details id="vAdv"><summary>Дополнительно</summary><div class="in">
      <div class="v-tip">Шаги: 8 на черновик + 3 на детали. У дистиллированной модели расписание зашито, менять его нельзя без потери качества.</div>
      <label class="l">CFG (оставь 1.0)<input id="vCfg" type="number" min="1" max="4" step="0.1" value="1"></label>
      <div class="v-warn" id="vCfgWarn" hidden>CFG выше 1.0 сильно увеличивает расход видеопамяти и время: у дистиллята смысла в нём нет.</div>
      <label class="switch"><input id="vAudio" type="checkbox" checked> звук (модель делает его вместе с видео)</label>
    </div></details>

    <div class="v-box" id="vHint"></div>
    <div class="v-box bad" id="vMissing" hidden></div>

    <div class="go-wrap" id="vGoWrap">
      <button class="go" id="vGo" type="submit"><span class="lbl"><svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg><span id="vGoText">Создать видео</span></span></button>
      <button class="cancel-btn" id="vCancel" type="button"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="5" y="5" width="14" height="14" rx="2.5"/></svg><span>Отменить</span></button>
    </div>
    <div id="vStatus" role="status" style="text-align:center;font-size:13px;color:var(--mute);min-height:18px"></div>
  </form>

  <div class="right"><section class="card results">
    <div class="v-stage" id="vStage">
      <div class="v-empty" id="vEmpty">Здесь появится видео<small>Файлы сохраняются в E:\\ComfyUI\\ComfyUI\\output\\video</small></div>
      <video id="vPlayer" controls loop playsinline hidden></video>
      <div class="v-prog" id="vProg" hidden>
        <div class="t"><span id="vProgLbl"></span><span id="vProgTime"></span></div>
        <div class="bar-wrap"><div class="bar-fill" id="vProgBar"></div></div>
      </div>
    </div>
    <div class="v-box bad" id="vErr" hidden></div>
    <div class="v-actions" id="vActions" hidden>
      <a id="vSave" download>Сохранить</a>
      <button type="button" id="vFolder">Открыть папку</button>
      <button type="button" id="vReuse">Те же настройки</button>
      <button type="button" id="vRegen">Ещё раз, новый seed</button>
      <button type="button" class="del" id="vDel">Удалить</button>
    </div>
    <div class="v-meta" id="vMeta"></div>
    <div class="v-queue" id="vQueue"></div>
    <div class="v-hist-h"><h3>История</h3><span id="vHistN"></span></div>
    <div id="vHist"></div>
  </section></div>`;
}

/* ---------- form state ---------- */
const sizeOk = n => Number.isInteger(n) && n >= 256 && n <= 1920 && n % 64 === 0;
const frames = () => Math.round($('vSec').value * $('vFps').value / 8) * 8 + 1;   // LTX wants 8k+1 frames
const settings = () => ({
  mode: $('vMode').value, prompt: $('vPrompt').value.trim(), negative: $('vNeg').value.trim(), width: +$('vW').value, height: +$('vH').value,
  seconds: +$('vSec').value, fps: +$('vFps').value, frames: frames(), seed: +$('vSeed').value, cfg: +$('vCfg').value, audio: $('vAudio').checked,
  unet: $('vUnet').value, clip: $('vClip').value, aspect: $('vAsp').value, size: $('vSize').value});

function pickPreset() {
  const [w, h] = ASPECTS[$('vAsp').value][+$('vSize').value];
  $('vW').value = w; $('vH').value = h; refresh();
}
function refresh() {
  const w = +$('vW').value, h = +$('vH').value, f = frames();
  $('vW').classList.toggle('bad', !sizeOk(w)); $('vH').classList.toggle('bad', !sizeOk(h));
  $('vSizeNote').textContent = sizeOk(w) && sizeOk(h) ? `${w}×${h}: сначала считается вдвое меньше, потом увеличивается ×2, поэтому размер кратен 64`
                                                      : 'Размер должен быть кратен 64, от 256 до 1920';
  $('vSecVal').textContent = `${(+$('vSec').value).toFixed(1)} с`;
  $('vFrames').textContent = `${f} кадров · ${(f / $('vFps').value).toFixed(1)} с при ${$('vFps').value} к/с`;
  $('vCfgWarn').hidden = !(+$('vCfg').value > 1);
  hint(w, h, f);
  put('form', JSON.stringify({asp:$('vAsp').value, size:$('vSize').value, w, h, sec:$('vSec').value, fps:$('vFps').value, audio:$('vAudio').checked, cfg:$('vCfg').value, tr:$('vTr').checked}));
}
// fit estimate from the smoke tests (ltx/config.json): cost grows with width*height*frames
function hint(w, h, f) {
  const c = V.cfg, box = $('vHint');
  if (!sizeOk(w) || !sizeOk(h)) { box.className = 'v-box warn'; box.textContent = 'Укажи правильный размер, тогда покажу оценку.'; return; }
  const cost = w * h * f / 1e6, cfgMul = Math.max(1, +$('vCfg').value) ** 1.5;
  const used = cost * cfgMul;
  if (!c.calibrated) { box.className = 'v-box'; box.textContent = `≈ ${cost.toFixed(1)} Мпикс·кадров. Оценка памяти появится после замеров на этой видеокарте.`; return; }
  const t = c.sec_per_mpxf ? ` · около ${Math.round((c.sec_base + used * c.sec_per_mpxf) / 6) / 10} мин, не считая первой загрузки моделей` : '';
  if (used <= c.safe) { box.className = 'v-box ok'; box.innerHTML = `<b>Поместится.</b> ${cost.toFixed(0)} из проверенных ${c.safe} Мпикс·кадров${t}`; }
  else if (used <= c.max) { box.className = 'v-box warn'; box.innerHTML = `<b>Выше проверенного.</b> Я проверял до ${c.safe} Мпикс·кадров, сейчас ${used.toFixed(0)}: может сработать, а может не хватить памяти. Если упадёт, уменьши размер или длительность.`; }
  else { box.className = 'v-box bad'; box.innerHTML = `<b>Скорее всего не хватит памяти.</b> Проверено до ${c.safe} Мпикс·кадров, сейчас ${used.toFixed(0)}. Уменьши размер или длительность.`; }
}

async function loadFiles() {
  let d; try { d = await j('/api/ltx/models'); } catch { d = {files:{unet:[],clip:[],vae:[],audio_vae:[],upscaler:[]}}; }
  V.files = d.files;
  // on this setup the GGUF Gemma encoder follows prompts poorly (measured), so the official int8 one is listed first and is the default
  const clips = [...d.files.clip].sort((x, y) => x.endsWith('.gguf') - y.endsWith('.gguf'));
  const fill = (id, list, key) => { const s = $(id), want = get(key, ''); s.innerHTML = list.length ? list.map(n => `<option value="${esc(n)}">${esc(n.replace(/^gemma4-12b-with-proj-ltx-2.5-/, '').replace(/\.(gguf|safetensors)$/, ''))}${n.endsWith('.gguf') ? ' (хуже следует промпту)' : ''}</option>`).join('') : '<option value="">нет файлов</option>'; if (list.includes(want)) s.value = want; };
  fill('vUnet', d.files.unet, 'unet'); fill('vClip', clips, 'clip');
  const need = [['unet', 'видео-модель (.gguf)', 'ComfyUI\\models\\unet'], ['clip', 'текстовый энкодер Gemma 4', 'ComfyUI\\models\\text_encoders'], ['vae', 'видео-VAE', 'ComfyUI\\models\\vae'],
                ['audio_vae', 'аудио-VAE', 'ComfyUI\\models\\vae'], ['upscaler', 'апскейлер латентов ×2', 'ComfyUI\\models\\latent_upscale_models']];
  const miss = need.filter(([k]) => !d.files[k].length);
  const box = $('vMissing');
  box.hidden = !miss.length;
  box.innerHTML = miss.length ? `<b>Не хватает файлов:</b><ul>${miss.map(([, n, p]) => `<li>${n} → <code>${p}</code></li>`).join('')}</ul>` : '';
  V.ready = !miss.length;
  $('vGo').disabled = !V.ready;
}
async function loadTemplates() {
  for (const m of ['t2v', 'i2v']) if (!V.tpl[m]) V.tpl[m] = await j(`/ltx/ltx_${m}_api.json`);
  try { V.cfg = await j('/ltx/config.json'); } catch { V.cfg = {}; }
  $('vSec').max = V.cfg.max_seconds || 10;
}

/* ---------- workflow ---------- */
function fillTpl(node, vars) {
  if (Array.isArray(node)) return node.map(n => fillTpl(n, vars));
  if (node && typeof node === 'object') return Object.fromEntries(Object.entries(node).map(([k, v]) => [k, fillTpl(v, vars)]));
  const m = typeof node === 'string' && node.match(/^\{\{(\w+)\}\}$/);
  return m ? vars[m[1]] : node;
}
function workflow(s, image) {
  const f = V.files, g = fillTpl(V.tpl[s.mode], {
    unet: s.unet, clip: s.clip, vae: f.vae[0], audio_vae: f.audio_vae[0], upscaler: f.upscaler[0], prompt: s.enPrompt, negative: s.enNeg || NEG,
    fps: s.fps, half_width: s.width / 2, half_height: s.height / 2, length: s.frames, seed: s.seed, cfg: s.cfg, image});
  if (!s.unet.endsWith('.gguf')) g['1'] = {class_type:'UNETLoader', inputs:{unet_name:s.unet, weight_dtype:'default'}};
  if (s.clip.endsWith('.gguf')) g['2'] = {class_type:'CLIPLoaderGGUF', inputs:{clip_name:s.clip, type:'ltxv'}};
  if (!s.audio) delete g['37'].inputs.audio;
  return g;
}

/* ---------- submit / queue / progress ---------- */
const say = (t, err) => { $('vStatus').textContent = t; $('vStatus').style.color = err ? 'var(--red)' : ''; };
function showErr(html) { $('vErr').hidden = !html; $('vErr').innerHTML = html || ''; }
function errText(type, msg) {
  const m = `${type} ${msg}`.toLowerCase();
  if (m.includes('out of memory') || m.includes('outofmemory') || m.includes('alloc')) {
    return '<b>Не хватило видеопамяти.</b><ul><li>уменьши размер (например «Малый») или длительность</li><li>выбери модель поменьше (Q4_K_S или Q3_K_M)</li><li>закрой игры и другие программы на видеокарте</li></ul>';
  }
  return `<b>ComfyUI вернул ошибку.</b><br>${esc(String(msg).split('\n')[0])}`;
}

async function submit(s) {
  showErr('');
  if (!V.ready) { say('Не хватает файлов моделей, список выше', true); return; }
  if (!s.prompt) { say('Введи промпт', true); $('vPrompt').focus(); return; }
  if (!sizeOk(s.width) || !sizeOk(s.height)) { say('Размер: кратно 64, от 256 до 1920', true); return; }
  if (s.mode === 'i2v' && !$('vPhoto').files[0] && !s.imageName) { say('Выбери стартовое фото', true); return; }
  $('vGo').disabled = true;
  try {
    s.enPrompt = s.prompt; s.enNeg = s.negative;
    if ($('vTr').checked && (CYR.test(s.prompt) || CYR.test(s.negative))) {
      say('Перевожу…');
      [s.enPrompt, s.enNeg] = await translate([s.prompt, s.negative]);
      $('vTrOut').textContent = 'EN: ' + s.enPrompt; $('vTrOut').hidden = false;
    } else $('vTrOut').hidden = true;
    if (s.mode === 'i2v') s.imageName = $('vPhoto').files[0] ? await uploadPhoto($('vPhoto').files[0]) : s.imageName;
    say(V.jobs.length ? 'Добавляю в очередь…' : 'Запускаю ComfyUI и отправляю задачу… (первый запуск до минуты)');
    const prog = await j('/api/ltx/progress').catch(() => ({}));
    const r = await post('/prompt', {prompt: workflow(s, s.imageName), client_id: prog.cid});
    V.jobs.push({pid: r.prompt_id, s});
    say('');
    startLoop();
  } catch (e) {
    const b = e.body;
    if (e.status === 502) say('ComfyUI не запущен и не смог стартовать. Смотри logs\\comfyui.out.log', true);
    else if (b?.node_errors && Object.keys(b.node_errors).length) {
      const first = Object.values(b.node_errors)[0];
      showErr(`<b>ComfyUI отклонил задачу.</b><br>${esc(first.errors?.[0]?.message || '')} ${esc(first.errors?.[0]?.extra_info?.input_name || '')}<br>Проверь, что выбранные файлы моделей на месте.`);
    } else say(String(e.message).includes('Failed to fetch') ? 'Нет связи с сервером Студии' : e.message, true);
  } finally { $('vGo').disabled = !V.ready; }
}

function startLoop() {
  syncBusy();
  if (!V.timer) V.timer = setInterval(tick, 500);
}
function syncBusy() {
  const busy = V.jobs.length > 0;
  $('vGoWrap').classList.toggle('busy', busy);
  $('vGoText').textContent = busy ? 'В очередь' : 'Создать видео';
  $('vQueue').textContent = V.jobs.length > 1 ? `В очереди ещё: ${V.jobs.length - 1}` : '';
  $('vProg').hidden = !busy;
  if (busy) { $('vPlayer').pause(); }
}
let ticking = false;
async function tick() {
  if (ticking || !V.jobs.length) return;
  ticking = true;
  const job = V.jobs[0];
  try {
    const [p, h] = await Promise.all([j('/api/ltx/progress'), j('/history/' + job.pid).catch(() => ({}))]);
    const item = h[job.pid];
    if (item?.status?.completed || item?.status?.status_str === 'error') return await finish(job, item);
    const mine = p.pid === job.pid && p.state === 'running';
    if (mine && !job.t0) job.t0 = Date.now();
    if (job.t0 && !mine && p.pid === job.pid && p.state === 'interrupted') return await finish(job, {status:{status_str:'interrupted'}});
    let label = job.t0 ? 'Подготовка…' : 'Ждёт очереди…', pct = V.pct;
    if (mine && p.node && PHASE[p.node]) {
      const [name, base, share] = PHASE[p.node], [val, max] = p.nodes?.[p.node] || [0, 0];
      pct = Math.max(pct, base + (max > 1 ? share * val / max : 0));
      label = name + (max > 1 ? ` · шаг ${val}/${max}` : '');
    }
    V.pct = pct;
    $('vProgBar').style.width = pct + '%';
    $('vProgLbl').textContent = `${label} · ${Math.round(pct)}%`;
    $('vProgTime').textContent = job.t0 ? `${Math.round((Date.now() - job.t0) / 1000)} с` : '';
  } catch { /* server hiccup: try again on the next tick */ } finally { ticking = false; }
}
async function finish(job, item) {
  V.jobs.shift(); V.pct = 0;
  $('vProgBar').style.width = '0%';
  const st = item.status?.status_str;
  if (st === 'success') {
    const out = Object.values(item.outputs || {}).flatMap(o => o.images || []).find(i => /\.mp4$/i.test(i.filename));
    if (out) {
      const meta = {...job.s, elapsed: job.t0 ? Math.round((Date.now() - job.t0) / 1000) : null, created: new Date().toISOString()};
      await post('/api/ltx/meta', {filename: out.filename, meta}).catch(() => {});
      await loadHistory();
      show({filename: out.filename, subfolder: out.subfolder || 'video', type: 'output', meta});
    } else showErr('<b>ComfyUI закончил, но видео не найдено.</b>');
  } else if (st === 'interrupted' || (item.status?.messages || []).some(m => m[0] === 'execution_interrupted')) say('Остановлено');
  else {
    const e = (item.status?.messages || []).find(m => m[0] === 'execution_error')?.[1] || {};
    showErr(errText(e.exception_type || '', e.exception_message || 'неизвестная ошибка'));
  }
  if (!V.jobs.length) { clearInterval(V.timer); V.timer = null; }
  syncBusy();
}
async function cancel() {
  if (!V.jobs.length) return;
  const queued = V.jobs.slice(1).map(x => x.pid);
  V.jobs.length = Math.min(V.jobs.length, 1);   // pending ones are dropped, the running one reports 'interrupted'
  say('Останавливаю…');
  if (queued.length) await post('/queue', {delete: queued}).catch(() => {});
  await post('/interrupt', {}).catch(() => {});
  syncBusy();
}

/* ---------- result + history ---------- */
const urlOf = x => `/view?filename=${encodeURIComponent(x.filename)}&subfolder=${encodeURIComponent(x.subfolder || '')}&type=${x.type || 'output'}`;
function show(x) {
  V.shown = x;
  const v = $('vPlayer'); v.src = urlOf(x); v.hidden = false; $('vEmpty').hidden = true; v.play().catch(() => {});
  $('vActions').hidden = false; $('vSave').href = urlOf(x); $('vSave').download = x.filename;
  const m = x.meta || {};
  $('vMeta').innerHTML = m.prompt ? `<q>${esc(m.prompt)}</q><br>${m.width}×${m.height} · ${m.frames} кадров, ${m.fps} к/с · seed ${m.seed} · ${esc(m.unet)}${m.elapsed ? ` · ${m.elapsed} с` : ''}` : esc(x.filename);
  document.querySelectorAll('.v-card').forEach(c => c.classList.toggle('on', c.dataset.f === x.filename));
}
async function loadHistory() {
  let list = []; try { list = await j('/api/ltx/history'); } catch {}
  V.hist = list;
  $('vHistN').textContent = list.length ? `${list.length}` : '';
  $('vHist').innerHTML = list.length ? '' : '<div class="v-tip">Пока пусто</div>';
  list.forEach(x => {
    const m = x.meta || {}, c = document.createElement('div');
    c.className = 'v-card'; c.dataset.f = x.filename; c.title = m.prompt || x.filename;
    c.innerHTML = `<video muted preload="metadata" src="${urlOf(x)}#t=0.1"></video><div class="cap">${esc((m.prompt || x.filename).slice(0, 60))}<br><i>${m.width ? `${m.width}×${m.height} · ${m.seconds} с` : ''}</i></div>`;
    c.onclick = () => show(x);
    $('vHist').appendChild(c);
  });
}
function applySettings(m) {
  $('vModeSeg').querySelector(`[data-value="${m.mode || 't2v'}"]`).click();
  $('vPrompt').value = m.prompt || ''; $('vNeg').value = m.negative || '';
  $('vW').value = m.width; $('vH').value = m.height; $('vSec').value = m.seconds; $('vFps').value = m.fps;
  $('vSeed').value = m.seed; $('vRand').checked = false; $('vCfg').value = m.cfg ?? 1; $('vAudio').checked = m.audio !== false;
  if (V.files?.unet.includes(m.unet)) $('vUnet').value = m.unet;
  if (V.files?.clip.includes(m.clip)) $('vClip').value = m.clip;
  refresh();
}

/* ---------- wiring ---------- */
function wire() {
  seg($('vModeSeg'), $('vMode'), v => { V.mode = v; fold('vFoldImg', v === 'i2v'); $('vPrompt').placeholder = v === 'i2v' ? 'Что должно происходить в кадре и как движется камера. Фото станет первым кадром.' : TIP; });
  seg($('vAspSeg'), $('vAsp'), pickPreset);
  seg($('vSizeSeg'), $('vSize'), pickPreset);
  ['vW', 'vH', 'vSec', 'vFps', 'vCfg', 'vAudio'].forEach(id => $(id).addEventListener('input', refresh));
  $('vTr').onchange = refresh;
  $('vRand').onchange = () => { $('vSeed').disabled = $('vRand').checked; };
  $('vSeed').disabled = true;

  const drop = $('vDrop'), input = $('vPhoto');
  drop.onclick = e => { if (!e.target.closest('.x')) input.click(); };
  drop.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); input.click(); } };
  drop.querySelector('.x').onclick = () => setFile(input, null);
  ['dragenter', 'dragover'].forEach(t => drop.addEventListener(t, e => { e.preventDefault(); drop.classList.add('over'); }));
  ['dragleave', 'drop'].forEach(t => drop.addEventListener(t, e => { e.preventDefault(); drop.classList.remove('over'); }));
  drop.addEventListener('drop', e => { const f = e.dataTransfer.files[0]; if (f?.type.startsWith('image/')) setFile(input, f); });
  input.onchange = () => { const f = input.files[0]; if (f) drop.querySelector('img').src = URL.createObjectURL(f); drop.classList.toggle('has', !!f); };

  $('vUnet').onchange = () => put('unet', $('vUnet').value);
  $('vClip').onchange = () => put('clip', $('vClip').value);
  $('vForm').onsubmit = e => {
    e.preventDefault();
    if ($('vRand').checked) $('vSeed').value = Math.floor(Math.random() * 2 ** 31);
    put('unet', $('vUnet').value);
    submit(settings());
  };
  $('vPrompt').addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') $('vForm').requestSubmit(); });
  $('vCancel').onclick = cancel;

  $('vFolder').onclick = () => post('/api/ltx/open-folder', {filename: V.shown.filename}).catch(e => say(e.message, true));
  $('vReuse').onclick = () => V.shown.meta?.prompt ? applySettings(V.shown.meta) : say('У этого файла нет сохранённых настроек', true);
  $('vRegen').onclick = () => {
    if (!V.shown.meta?.prompt) return say('У этого файла нет сохранённых настроек', true);
    applySettings(V.shown.meta);
    const s = settings(); s.seed = Math.floor(Math.random() * 2 ** 31); $('vSeed').value = s.seed;
    if (s.mode === 'i2v') s.imageName = V.shown.meta.imageName;
    submit(s);
  };
  $('vDel').onclick = async () => {
    if (!confirm(`Удалить ${V.shown.filename} с диска? Это нельзя отменить.`)) return;
    await post('/api/ltx/delete', {filename: V.shown.filename}).catch(e => say(e.message, true));
    $('vPlayer').removeAttribute('src'); $('vPlayer').hidden = true; $('vEmpty').hidden = false; $('vActions').hidden = true; $('vMeta').textContent = '';
    V.shown = null; loadHistory();
  };
}

window.initVideoTab = async function (root) {
  const saved = (() => { try { return JSON.parse(get('form', '{}')); } catch { return {}; } })();   // read before wire(): wiring saves defaults
  build(root); wire();
  if (saved.asp) { $('vAsp').value = saved.asp; $('vSize').value = saved.size; }
  await loadTemplates();
  await loadFiles();
  $('vAspSeg').querySelector(`[data-value="${$('vAsp').value}"]`).click();
  $('vSizeSeg').querySelector(`[data-value="${$('vSize').value}"]`).click();
  if (saved.w) { $('vW').value = saved.w; $('vH').value = saved.h; }
  $('vSec').value = saved.sec || 5; $('vFps').value = saved.fps || 24; $('vCfg').value = saved.cfg || 1;
  $('vAudio').checked = saved.audio !== false; $('vTr').checked = saved.tr !== false;
  refresh();
  loadHistory();
  // a job may still be running from before the page was reloaded
  try {
    const q = await j('/queue');
    for (const it of [...(q.queue_running || []), ...(q.queue_pending || [])]) if (it[2]?.['38']?.class_type === 'SaveVideo') V.jobs.push({pid: it[1], s: {}});
    if (V.jobs.length) startLoop();
  } catch {}
};
})();
