// shared snippets for the screens
const I = {
  back: '<svg class="ic" viewBox="0 0 24 24"><path d="m15 18-6-6 6-6"/></svg>',
  plus: '<svg class="ic" viewBox="0 0 24 24"><path d="M5 12h14"/><path d="M12 5v14"/></svg>',
  send: '<svg class="ic" viewBox="0 0 24 24"><path d="m5 12 7-7 7 7"/><path d="M12 19V5"/></svg>',
  shuffle: '<svg class="ic" viewBox="0 0 24 24"><path d="m18 14 4 4-4 4"/><path d="m18 2 4 4-4 4"/><path d="M2 18h1.973a4 4 0 0 0 3.3-1.7l5.454-8.6a4 4 0 0 1 3.3-1.7H22"/><path d="M2 6h1.972a4 4 0 0 1 3.6 2.2"/><path d="M22 18h-6.041a4 4 0 0 1-3.3-1.8l-.359-.45"/></svg>',
  chev: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="m9 18 6-6-6-6"/></svg>',
  chevd: '<svg class="ic" viewBox="0 0 24 24" style="width:14px;height:14px"><path d="m6 9 6 6 6-6"/></svg>',
  laptop: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9m16 0H4m16 0 1.28 2.55a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45L4 16"/></svg>',
  camera: '<svg class="ic" viewBox="0 0 24 24"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>',
  people: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  globe: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg>',
  search: '<svg class="ic" viewBox="0 0 24 24" style="width:18px;height:18px"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>',
  sparkle: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/></svg>',
  check: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="M20 6 9 17l-5-5"/></svg>',
  mic: '<svg class="ic" viewBox="0 0 24 24"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>',
  eye: '<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/></svg>',
};
function statusBar(){return `<div class="status"><span>9:41</span><span>●●● ᯤ ▮</span></div>`}
function fake(acc, rows){return `<div class="fake" style="--acc:${acc}">${rows}</div>`}
document.addEventListener('DOMContentLoaded',()=>{
  document.body.innerHTML=document.body.innerHTML.replace(/\{\{(\w+)\}\}/g,(m,k)=>k==='statusbar'?statusBar():(I[k]??m));
  document.querySelectorAll('[data-slop]').forEach(el=>{const spec=el.dataset.slop;const size=+(el.dataset.size||44);let d;try{d=JSON.parse(spec)}catch{d=spec}el.innerHTML=Slop.svg(d,size)});
  document.querySelectorAll('[data-fake]').forEach(el=>{el.innerHTML='<div class="fake" style="--acc:'+(el.dataset.acc||'#60A5FA')+'">'+el.dataset.fake.split(',').map(c=>'<i class="'+c+'"></i>').join('')+'</div>'+el.innerHTML});
});

I.x='<svg class="ic" viewBox="0 0 24 24" style="width:18px;height:18px"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>';
I.share='<svg class="ic" viewBox="0 0 24 24"><path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><polyline points="16 6 12 2 8 6"/><line x1="12" x2="12" y1="2" y2="15"/></svg>';
I.book='<svg class="ic" viewBox="0 0 24 24"><path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H19a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H6.5a1 1 0 0 1 0-5H20"/></svg>';
I.map='<svg class="ic" viewBox="0 0 24 24"><path d="M14.106 5.553a2 2 0 0 0 1.788 0l3.659-1.83A1 1 0 0 1 21 4.619v12.764a1 1 0 0 1-.553.894l-4.553 2.277a2 2 0 0 1-1.788 0l-4.212-2.106a2 2 0 0 0-1.788 0l-3.659 1.83A1 1 0 0 1 3 19.381V6.618a1 1 0 0 1 .553-.894l4.553-2.277a2 2 0 0 1 1.788 0z"/><path d="M15 5.764v15"/><path d="M9 3.236v15"/></svg>';
I.wheel='<svg class="ic" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="3"/><path d="M12 2v7"/><path d="m4.93 19.07 4.95-4.95"/><path d="m19.07 19.07-4.95-4.95"/></svg>';
I.hash='<svg class="ic" viewBox="0 0 24 24" style="width:16px;height:16px"><line x1="4" x2="20" y1="9" y2="9"/><line x1="4" x2="20" y1="15" y2="15"/><line x1="10" x2="8" y1="3" y2="21"/><line x1="16" x2="14" y1="3" y2="21"/></svg>';
I.server='<svg class="ic" viewBox="0 0 24 24"><rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/></svg>';
I.store='<svg class="ic" viewBox="0 0 24 24"><path d="m2 7 4.41-4.41A2 2 0 0 1 7.83 2h8.34a2 2 0 0 1 1.42.59L22 7"/><path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><path d="M15 22v-4a2 2 0 0 0-2-2h-2a2 2 0 0 0-2 2v4"/><path d="M2 7h20"/><path d="M22 7v3a2 2 0 0 1-2 2a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 16 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 12 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 8 12a2.7 2.7 0 0 1-1.59-.63.7.7 0 0 0-.82 0A2.7 2.7 0 0 1 4 12a2 2 0 0 1-2-2V7"/></svg>';
