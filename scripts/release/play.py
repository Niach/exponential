# usage: play.py <package> show | promote <versionCode> <name> | to <track> <versionCode> <name>
# Google Play publishing API with a service-account JSON ($PLAY_SERVICE_ACCOUNT_JSON).
# `promote` = alpha -> production; `to` = alpha -> any non-production track.
import json,time,jwt,urllib.request,sys
import os
K=json.load(open(os.environ.get('PLAY_SERVICE_ACCOUNT_JSON', os.path.expanduser('~/keystores/play-publisher.json'))))
PKG=sys.argv[1]; mode=sys.argv[2]
now=int(time.time())
a=jwt.encode({'iss':K['client_email'],'scope':'https://www.googleapis.com/auth/androidpublisher','aud':K['token_uri'],'iat':now,'exp':now+3600},K['private_key'],algorithm='RS256')
tok=json.load(urllib.request.urlopen(urllib.request.Request(K['token_uri'],data=f'grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={a}'.encode())))['access_token']
B=f'https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{PKG}'
def call(method,path,body=None):
  r=urllib.request.Request(B+path,method=method,data=json.dumps(body).encode() if body is not None else None,headers={'Authorization':'Bearer '+tok,'Content-Type':'application/json'})
  raw=urllib.request.urlopen(r).read(); return json.loads(raw) if raw else {}
e=call('POST','/edits',{})['id']
if mode=='show':
  for t in call('GET',f'/edits/{e}/tracks')['tracks']:
    print(t['track'],[(r.get('name'),r.get('versionCodes'),r.get('status'),r.get('userFraction')) for r in t.get('releases',[])])
  call('DELETE',f'/edits/{e}')
elif mode=='promote':
  vc=sys.argv[3]
  alpha=call('GET',f'/edits/{e}/tracks/alpha')
  rel=[r for r in alpha['releases'] if vc in r.get('versionCodes',[])][0]
  new={'name':sys.argv[4],'versionCodes':[vc],'status':'completed'}
  if rel.get('releaseNotes'): new['releaseNotes']=rel['releaseNotes']
  print(call('PUT',f'/edits/{e}/tracks/production',{'track':'production','releases':[new]}))
  print(call('POST',f'/edits/{e}:commit'))
elif mode=='to':
  # play.py PKG to <track> <vc> <name>: put alpha's release <vc> on <track> (completed); never production
  track,vc,name=sys.argv[3],sys.argv[4],sys.argv[5]
  assert track!='production'
  alpha=call('GET',f'/edits/{e}/tracks/alpha')
  rel=[r for r in alpha['releases'] if vc in r.get('versionCodes',[])][0]
  new={'name':name,'versionCodes':[vc],'status':'completed'}
  if rel.get('releaseNotes'): new['releaseNotes']=rel['releaseNotes']
  print(call('PUT',f'/edits/{e}/tracks/{track}',{'track':track,'releases':[new]}))
  print(call('POST',f'/edits/{e}:commit'))
