import sharp from "sharp"
const [out,w,...fs]=process.argv.slice(2); const W=+w
const ims=await Promise.all(fs.map(async f=>{const b=await sharp(f).resize({width:W}).toBuffer();const m=await sharp(b).metadata();return{b,h:m.height!}}))
const H=Math.max(...ims.map(i=>i.h))
await sharp({create:{width:(W+12)*ims.length,height:H,channels:3,background:"#555"}}).composite(ims.map((i,k)=>({input:i.b,left:k*(W+12),top:0}))).png().toFile(out)
