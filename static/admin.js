const $=s=>document.querySelector(s);
const tk=()=>sessionStorage.getItem('admin_token')||'';
const hd=()=>({'Authorization':`Bearer ${tk()}`,'Content-Type':'application/json'});

async function auth(){
  const t=$('#admin-token').value.trim();if(!t)return;
  sessionStorage.setItem('admin_token',t);
  const fb=$('#auth-fb');
  try{
    const r=await fetch('/api/admin/promo',{headers:{'Authorization':`Bearer ${t}`}});
    if(r.status===401){fb.className='fb-err';fb.textContent='Invalid token';sessionStorage.removeItem('admin_token');return}
    fb.className='fb-ok';fb.textContent='Connected';
    $('#auth-panel').classList.add('hidden');$('#dash').classList.remove('hidden');loadAll();
  }catch{fb.className='fb-err';fb.textContent='Connection failed'}
}
if(tk()){$('#admin-token').value=tk();auth()}

async function loadAll(){await Promise.all([loadPromos(),loadSeasons(),loadRegs(),loadHeaderSeason()])}

async function loadHeaderSeason(){
  try{
    const r=await fetch('/api/season');if(!r.ok)return;
    const s=await r.json();
    const n=String(s.id).padStart(2,'0');
    $('#hdr-sn').textContent='S'+n;
    document.title='Seasons S'+n+' // Admin';
  }catch{}
}

async function loadPromos(){
  try{
    const r=await fetch('/api/admin/promo',{headers:hd()});if(!r.ok)throw 0;
    const ps=await r.json(),tb=$('#promo-tbody');
    if(!ps.length){tb.innerHTML='<tr><td colspan="6">No promo codes</td></tr>';return}
    tb.innerHTML=ps.map(p=>{
      const u=p.max_uses?`${p.times_used}/${p.max_uses}`:`${p.times_used}`;
      const ac=p.active?'s-ok':'s-no',al=p.active?'Active':'Revoked';
      const b=p.active?`<button class="secondary" data-revoke="${p.code}">Revoke</button>`:'';
      return `<tr><td><code>${p.code}</code></td><td>${p.discount_percent}%</td><td>${p.grants_instant_player?'Yes':'No'}</td><td>${u}</td><td class="${ac}">${al}</td><td>${b}</td></tr>`;
    }).join('');
    tb.querySelectorAll('[data-revoke]').forEach(btn=>{
      btn.addEventListener('click',()=>revokePromo(btn.dataset.revoke));
    });
  }catch{$('#promo-tbody').innerHTML='<tr><td colspan="6">Failed to load</td></tr>'}
}

async function createPromo(e){
  e.preventDefault();const fb=$('#promo-fb');fb.textContent='';
  const body={code:$('#p-code').value.trim(),discount_percent:+$('#p-disc').value,
    grants_instant_player:$('#p-instant').checked};
  const mu=$('#p-max').value;if(mu)body.max_uses=+mu;
  try{
    const r=await fetch('/api/admin/promo',{method:'POST',headers:hd(),body:JSON.stringify(body)});
    const d=await r.json();
    if(!r.ok){fb.className='fb-err';fb.textContent=d.error||'Failed';return}
    fb.className='fb-ok';fb.textContent=`Created: ${d.code}`;$('#promo-form').reset();loadPromos();
  }catch{fb.className='fb-err';fb.textContent='Network error'}
}

async function revokePromo(code){
  if(!confirm(`Revoke promo "${code}"?`))return;
  try{const r=await fetch(`/api/admin/promo/${encodeURIComponent(code)}`,{method:'DELETE',headers:hd()});
    if(r.ok||r.status===204)loadPromos()}catch{}
}

async function loadSeasons(){
  try{const r=await fetch('/api/seasons');if(!r.ok)return;const ss=await r.json();
    $('#f-season').innerHTML='<option value="">All</option>'+
      ss.map(s=>`<option value="${s.id}">Season ${s.id} (${s.status})</option>`).join('');
  }catch{}
}

let allRegs=[];

async function loadRegs(){
  try{
    const sid=$('#f-season').value,st=$('#f-status').value;
    let url='/api/admin/registrations';const p=[];
    if(sid)p.push(`season_id=${sid}`);if(st)p.push(`status=${st}`);
    if(p.length)url+='?'+p.join('&');
    const r=await fetch(url,{headers:hd()});if(!r.ok)throw 0;
    allRegs=await r.json();
    renderRegs();
  }catch{$('#reg-tbody').innerHTML='<tr><td colspan="9">Failed to load</td></tr>'}
}

function renderRegs(){
  const q=$('#f-search').value.trim().toLowerCase();
  const filtered=q?allRegs.filter(r=>
    r.factorio_name.toLowerCase().includes(q)||
    r.id.toLowerCase().includes(q)||
    (r.promo_code&&r.promo_code.toLowerCase().includes(q))
  ):allRegs;
  const tb=$('#reg-tbody');
  if(!filtered.length){tb.innerHTML='<tr><td colspan="9">No registrations</td></tr>';return}
  tb.innerHTML=filtered.map(r=>{
    const sc=r.status==='confirmed'?'s-ok':r.status==='awaiting_payment'?'s-wait':'s-no';
    const ts=r.confirmed_at||r.created_at;
    const shortId=r.id.substring(0,8);
    const revBtn=r.status!=='expired'?`<button class="secondary danger" data-revoke-reg="${r.id}" data-revoke-name="${r.factorio_name}">Revoke</button>`:'';
    return `<tr><td><code title="${r.id}">${shortId}</code></td><td>${r.factorio_name}</td><td>${r.season_id}</td><td class="${sc}">${r.status}</td>
      <td>${r.access_tier==='instant_player'?'Player':'Spectator'}</td>
      <td><code>${r.amount_wei}</code></td><td>${r.promo_code||'\u2014'}</td>
      <td>${new Date(ts).toLocaleString()}</td><td>${revBtn}</td></tr>`;
  }).join('');
  tb.querySelectorAll('[data-revoke-reg]').forEach(btn=>{
    btn.addEventListener('click',()=>revokeRegistration(btn.dataset.revokeReg,btn.dataset.revokeName));
  });
}

async function revokeRegistration(id,name){
  if(!confirm(`Revoke registration for "${name}"?\n\nThis will expire the registration and remove them from the whitelist.`))return;
  try{
    const r=await fetch(`/api/admin/registrations/${encodeURIComponent(id)}`,{method:'DELETE',headers:hd()});
    if(r.ok||r.status===204)loadRegs();
  }catch{}
}

async function forceRotate(){
  if(!confirm('Force season rotation?\n\n1. Archive current season\n2. Carry forward confirmed players\n3. Generate fresh map\n4. Start new season\n\nThis cannot be undone.'))return;
  const btn=$('#rotate-btn'),fb=$('#rotate-fb');
  btn.disabled=true;btn.textContent='Rotating...';fb.textContent='';
  try{
    const r=await fetch('/api/admin/rotate',{method:'POST',headers:hd()});const d=await r.json();
    if(!r.ok){fb.className='fb-err';fb.textContent=d.error||'Failed'}
    else{fb.className='fb-ok';fb.textContent='Season rotated!';loadAll()}
  }catch{fb.className='fb-err';fb.textContent='Network error'}
  finally{btn.disabled=false;btn.textContent='Force Rotate Season'}
}

$('#auth-btn').addEventListener('click',auth);
$('#promo-form').addEventListener('submit',createPromo);
$('#rotate-btn').addEventListener('click',forceRotate);
$('#f-season').addEventListener('change',loadRegs);
$('#f-status').addEventListener('change',loadRegs);
$('#f-search').addEventListener('input',renderRegs);
