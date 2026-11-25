import { chromium } from "@playwright/test"
import { DEMO_EMAIL, DEMO_PASSWORD } from "./screenshot-demo"
const B="https://localhost:3066", O="/tmp/dbe6/shots"
const b=await chromium.launch()
async function ctx(mobile:boolean){
  const c=await b.newContext({ignoreHTTPSErrors:true,viewport:mobile?{width:390,height:844}:{width:1440,height:900},deviceScaleFactor:2,hasTouch:mobile,isMobile:mobile,colorScheme:"dark"})
  const p=await c.newPage(); await p.goto(B+"/auth/login",{waitUntil:"domcontentloaded"})
  await p.waitForTimeout(6000)
  const em=p.getByText("Continue with email"); if(await em.count()) {await em.first().click().catch(()=>{}) }
  await p.waitForSelector("#email",{timeout:60000}); await p.fill("#email",DEMO_EMAIL); await p.fill("#password",DEMO_PASSWORD); await p.keyboard.press("Enter")
  await p.waitForURL(u=>!u.pathname.includes("login"),{timeout:60000}); await p.waitForTimeout(3000)
  return p }
const only=process.argv[2]??"all"
if(only!=="wide"){
const p=await ctx(true)
const shot=async(n:string)=>{await p.screenshot({path:`${O}/${n}.png`});console.log(n)}
const go=async(u:string,t=6000)=>{await p.goto(B+u,{waitUntil:"domcontentloaded"});await p.waitForTimeout(t)}
await go("/teams",9000); await shot("wm-teams-open")
await p.evaluate(()=>{const s=document.querySelector('[data-testid=phone-teams]')!;s.scrollTop=260}); await p.waitForTimeout(500); await shot("wm-teams-scrolled")
await p.evaluate(()=>{document.querySelector('[data-testid=phone-teams]')!.scrollTop=0})
for(const r of await p.locator('[data-testid=sidebar-team-row] > [data-sidebar=menu-button]').all()) await r.click()
await p.waitForTimeout(600); await shot("wm-teams-folded")
await p.locator('[data-testid=sidebar-team-row] > [data-sidebar=menu-button]').first().click(); await p.waitForTimeout(500)
await p.locator('[data-testid=sidebar-board-row] [data-menu-id]').first().evaluate(e=>{const r=e.getBoundingClientRect();e.dispatchEvent(new PointerEvent("contextmenu",{bubbles:true,cancelable:true,clientX:r.x+60,clientY:r.y+20,pointerType:"touch"} as never))})
await p.waitForTimeout(900); await shot("wm-teams-menu"); await p.keyboard.press("Escape"); await p.waitForTimeout(500)
await p.getByTestId("sidebar-team-new-button").first().click(); await p.waitForTimeout(800); await shot("wm-team-new"); await p.keyboard.press("Escape"); await p.waitForTimeout(600)
await p.locator('[data-testid=sidebar-board-row] a').first().click(); await p.waitForTimeout(5000); await shot("wm-board"); await p.evaluate(()=>{for(const e of document.querySelectorAll("*")) if(e.scrollHeight>e.clientHeight+40&&getComputedStyle(e).overflowY==="auto") e.scrollTop=300}); await p.waitForTimeout(400); await shot("wm-board-scrolled")
await go("/chats"); await shot("wm-chats"); await p.evaluate(()=>{for(const e of document.querySelectorAll('*')) if(e.scrollHeight>e.clientHeight+40&&getComputedStyle(e).overflowY==='auto') e.scrollTop=300}); await p.waitForTimeout(500); await shot("wm-chats-scrolled")
await go("/assistant",8000); await shot("wm-assistant")
await go("/settings"); await p.evaluate(()=>{document.querySelector('[data-testid=phone-settings]')!.scrollTop=160}); await p.waitForTimeout(400); await shot("wm-settings-scrolled")
await go("/settings/notifications"); await shot("wm-settings-page")
await go("/inbox"); await shot("wm-inbox")
}
if(only!=="mobile"){
const p=await ctx(false)
const shot=async(n:string)=>{await p.screenshot({path:`${O}/${n}.png`});console.log(n)}
const go=async(u:string,t=6000)=>{await p.goto(B+u,{waitUntil:"domcontentloaded"});await p.waitForTimeout(t)}
await go("/home",9000); await p.getByTestId("sidebar-team-row").first().hover(); await p.waitForTimeout(400); await shot("w-sidebar")
await p.locator('[data-testid=sidebar-board-row] a').first().click(); await p.waitForTimeout(5000); await shot("w-board")
await p.getByTestId("sidebar-assistant").click(); await p.waitForTimeout(6000); await shot("w-assistant")
await p.locator('[data-testid=agents-row]').first().click(); await p.waitForTimeout(6000); await shot("w-agent")
}
await b.close()
