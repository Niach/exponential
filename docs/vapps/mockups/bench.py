import torch, time, sys, os
from diffusers import AutoPipelineForText2Image
os.makedirs('/tmp/exp-1174/gen', exist_ok=True)
t=time.time()
pipe = AutoPipelineForText2Image.from_pretrained('stabilityai/sd-turbo', torch_dtype=torch.float16, variant='fp16').to('mps')
print(f'load {time.time()-t:.1f}s', flush=True)
style = ', cute mascot sticker, simple rounded shapes, big friendly eyes, soft pastel gradient, flat vector illustration, centered, plain white background'
neg = 'text, watermark, realistic, photo, blurry, scary, multiple characters'
subjects = {'towel':'a stoned sleepy bath towel character with half-closed eyes','cat':'a round orange cat character','cloud':'a little cloud character wearing a beret','cactus':'a cactus character wearing sunglasses','toaster':'a happy toaster character','mushroom':'a tiny mushroom character with a bow tie'}
g = torch.Generator('mps').manual_seed(7)
for steps in (1, 4):
    for k, s in subjects.items():
        t=time.time()
        img = pipe(prompt=s+style, negative_prompt=neg, num_inference_steps=steps, guidance_scale=0.0, height=512, width=512, generator=g).images[0]
        img.save(f'/tmp/exp-1174/gen/{k}-{steps}.png')
        print(f'{k} steps={steps} {time.time()-t:.2f}s', flush=True)
