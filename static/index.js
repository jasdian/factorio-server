const $=s=>document.querySelector(s);

async function loadSeason(){
  try{
    const r=await fetch('/api/season');if(!r.ok)throw 0;
    const s=await r.json();
    $('#s-title').textContent=`Season ${s.id} \u2014 ${s.status}`;
    $('#p-count').textContent=s.player_count;
    $('#sp-count').textContent=s.spectator_count;
    if(s.factorio_version)$('#f-version').textContent=s.factorio_version;
    if(s.base_fee_wei){
      const eth=(BigInt(s.base_fee_wei)*BigInt(1000000))/BigInt('1000000000000000000');
      const ethStr=(Number(eth)/1e6).toFixed(6).replace(/0+$/,'').replace(/\.$/,'')+' ETH';
      $('#reg-fee').textContent=ethStr;
    }
    !function tick(){
      const d=new Date(s.ends_at)-Date.now();
      if(d<=0){$('#countdown').textContent='SEASON ENDED';return}
      const D=Math.floor(d/864e5),h=Math.floor(d%864e5/36e5),
            m=Math.floor(d%36e5/6e4),sec=Math.floor(d%6e4/1e3);
      $('#countdown').textContent=(D?D+'d ':'')+ `${h}h ${m}m ${sec}s`;
      setTimeout(tick,1000);
    }();
  }catch{$('#s-title').textContent='Season Unavailable'}
}

async function loadSeasons(){
  try{
    const r=await fetch('/api/seasons');if(!r.ok)throw 0;
    const list=await r.json(),tb=$('#s-tbody');
    if(!list.length){tb.innerHTML='<tr><td colspan="5">No seasons yet</td></tr>';return}
    tb.innerHTML=list.map(s=>`<tr><td>${s.id}</td><td>${s.status}</td>
      <td>${new Date(s.started_at).toLocaleDateString()}</td>
      <td>${new Date(s.ends_at).toLocaleDateString()}</td>
      <td>${s.has_map_download?`<a href="/api/maps/${s.id}" download>Download</a>`:'\u2014'}</td></tr>`).join('');
  }catch{$('#s-tbody').innerHTML='<tr><td colspan="5">Unable to load</td></tr>'}
}

$('#reg-form').addEventListener('submit',async e=>{
  e.preventDefault();
  const btn=$('#reg-btn');btn.disabled=true;btn.textContent='Processing...';
  const body={factorio_name:$('#f_name').value.trim()};
  const eth=$('#f_eth').value.trim();if(eth)body.eth_address=eth;
  const promo=$('#f_promo').value.trim();if(promo)body.promo_code=promo;
  const div=$('#reg-result');
  try{
    const r=await fetch('/api/register',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
    let d;
    try{d=await r.json()}catch{
      const txt=await r.clone().text().catch(()=>'');
      div.className='msg err';div.innerHTML=`Server error (${r.status}): ${txt||'non-JSON response'}`;
      div.classList.remove('hidden');return;
    }
    if(!r.ok){div.className='msg err';div.innerHTML=d.error||'Registration failed'}
    else{
      const bc=d.access_tier==='instant_player'?'badge-p':'badge-s';
      const bl=d.access_tier==='instant_player'?'Player':'Spectator';
      let h=`<p><strong>Registration successful!</strong></p>
        <p>ID: <code>${d.registration_id}</code></p>
        <p>Access: <span class="badge ${bc}">${bl}</span></p>`;
      if(d.deposit_address)h+=`<p>Send exactly <strong>${d.amount_wei}</strong> wei to:</p><p><code>${d.deposit_address}</code></p>`;
      else h+=`<p class="s-ok">${d.message}</p>`;
      div.className='msg ok';div.innerHTML=h;
    }
    div.classList.remove('hidden');
  }catch(e){div.className='msg err';div.innerHTML=`Network error: ${e.message}`;div.classList.remove('hidden')}
  finally{btn.disabled=false;btn.textContent='Register'}
});

$('#chk-form').addEventListener('submit',async e=>{
  e.preventDefault();
  const id=$('#reg_id').value.trim(),div=$('#chk-result');
  try{
    const r=await fetch(`/api/register/${encodeURIComponent(id)}`);
    if(!r.ok){div.className='msg err';div.innerHTML='Registration not found'}
    else{
      const d=await r.json();
      const sc=d.status==='confirmed'?'s-ok':d.status==='awaiting_payment'?'s-wait':'s-no';
      const bc=d.access_tier==='instant_player'?'badge-p':'badge-s';
      const bl=d.access_tier==='instant_player'?'Player':'Spectator';
      let ex;
      if(d.status==='confirmed'&&d.access_tier==='instant_player')ex='Full player access this season + carry forward.';
      else if(d.status==='confirmed')ex='Spectator this season. Full player next season.';
      else if(d.status==='awaiting_payment')ex=`Send exactly ${d.amount_wei} wei to complete registration.`;
      else ex='This registration has expired.';
      div.className='msg ok';
      div.innerHTML=`<p><strong>${d.factorio_name}</strong> \u2014 Season ${d.season_id}</p>
        <p>Status: <span class="${sc}">${d.status}</span></p>
        <p>Tier: <span class="badge ${bc}">${bl}</span></p>
        ${d.tx_hash?`<p>TX: <code>${d.tx_hash}</code></p>`:''}
        <p><em>${ex}</em></p>`;
    }
    div.classList.remove('hidden');
  }catch{div.className='msg err';div.innerHTML='Network error';div.classList.remove('hidden')}
});

loadSeason();loadSeasons();
