import subprocess,time,xml.etree.ElementTree as ET,json,re,os,argparse
from pathlib import Path
parser=argparse.ArgumentParser(description="Safe-area checks on DISPOSABLE rooted Android emulators only. Replaces Sol Flow test preferences.")
parser.add_argument('--serial', required=True)
parser.add_argument('--apk', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
config=parser.parse_args()
ADB=os.environ.get('ADB', 'adb')
OUT=config.output;OUT.mkdir(parents=True,exist_ok=True)
def adb(serial,*args):
 assert serial == config.serial and serial.startswith('emulator-'), 'Physical devices are never allowed'
 return subprocess.check_output([ADB,'-s',serial,*args])
def sh(s,*args):return adb(s,'shell',*args).decode().strip()
def setup(s):
 sh(s,'am','force-stop','com.handy.voice')
 sh(s,'mkdir','-p','/data/user/0/com.handy.voice/shared_prefs')
 prefs='<?xml version="1.0" encoding="utf-8"?><map><boolean name="intro_shown" value="true"/><int name="last_seen_version" value="45"/></map>'
 local=OUT/'handy.xml';local.write_text(prefs)
 adb(s,'push',str(local),'/data/local/tmp/solflow-test-prefs.xml')
 uid=sh(s,'stat','-c','%u','/data/user/0/com.handy.voice')
 sh(s,'cp','/data/local/tmp/solflow-test-prefs.xml','/data/user/0/com.handy.voice/shared_prefs/handy.xml')
 sh(s,'chown','-R',uid+':'+uid,'/data/user/0/com.handy.voice/shared_prefs')
 sh(s,'pm','grant','com.handy.voice','android.permission.RECORD_AUDIO')
 sh(s,'input','keyevent','KEYCODE_WAKEUP');sh(s,'wm','dismiss-keyguard')
 sh(s,'settings','put','system','screen_off_timeout','1800000')
 sh(s,'am','start','-n','com.handy.voice/.MainActivity')
 time.sleep(2)
def capture(s,name):
 sh(s,'uiautomator','dump','/sdcard/ui.xml')
 data=adb(s,'exec-out','cat','/sdcard/ui.xml');(OUT/(name+'.xml')).write_bytes(data)
 (OUT/(name+'.png')).write_bytes(adb(s,'exec-out','screencap','-p'))
 nodes=[n.attrib for n in ET.fromstring(data).iter('node')]
 interesting=[{k:n.get(k) for k in ('text','content-desc','resource-id','bounds')} for n in nodes if n.get('text') or n.get('content-desc')]
 print(name, 'captured')
 return nodes

# Requiring a named disposable AVD prevents accidental use of a personal emulator.
s=config.serial
avd=adb(s,'emu','avd','name').decode().splitlines()[0]
assert avd.startswith('solflow_edges'), 'Create a disposable AVD named solflow_edges* first'
assert sh(s,'id','-u') == '0', 'Use adb root on this disposable emulator first'
adb(s,'install','-r',str(config.apk.resolve()))
sh(s,'settings','put','system','accelerometer_rotation','0')
sh(s,'settings','put','system','user_rotation','0');time.sleep(2)
setup(s)
def bounds(n):return list(map(int,re.findall(r'\d+',n['bounds'])))
def node(ns,id):return next(n for n in ns if n.get('resource-id','').endswith('/'+id))
def tap(n):
 a,b,c,d=bounds(n);sh(s,'input','tap',str((a+c)//2),str((b+d)//2));time.sleep(1)
ns=capture(s,'main')
density=int(re.findall(r'\d+',sh(s,'wm','density'))[-1])/160
edge=round(32*density)
assert bounds(node(ns,'modelName'))[0]>=edge, 'Baseline padding missing'
for id in ['navDictation','navMeetings','navHistory','navModels']:
 assert bounds(node(ns,id))[0]>=edge, id
# Exercise the actual navigation and settings control, including immediate relayout.
tap(node(ns,'menuDictation'));ns=capture(s,'drawer')
assert bounds(node(ns,'drawerVersion'))[0]>=round(24*density)
sh(s,'am','start','-n','com.handy.voice/.SettingsActivity');time.sleep(1)
ns=capture(s,'settings')
label=next(n for n in ns if n.get('text') in ('Extra screen edge spacing','Увеличенные отступы у краёв'))
tap(label);capture(s,'settings-extra')
sh(s,'input','keyevent','KEYCODE_BACK');time.sleep(1)
ns=capture(s,'main-extra')
assert bounds(node(ns,'modelName'))[0]>=edge+round(16*density)
api=int(sh(s,'getprop','ro.build.version.sdk'))
if api>=30:
 sh(s,'cmd','overlay','enable','--user','0','com.android.internal.display.cutout.emulation.waterfall')
 sh(s,'am','force-stop','com.handy.voice');sh(s,'am','start','-n','com.handy.voice/.MainActivity');time.sleep(2)
 ns=capture(s,'waterfall')
 assert bounds(node(ns,'modelName'))[0]>edge+round(16*density)

def frame(s):
 d=sh(s,'dumpsys','window','windows')
 chunks=[c for c in d.split('  Window #') if 'package=com.handy.voice appop=SYSTEM_ALERT_WINDOW' in c and 'mViewVisibility=0x0' in c]
 assert len(chunks)==1, 'No visible dictation overlay'
 c=chunks[0]
 m=re.search(r'parent=\[(\d+),(\d+)\]\[(\d+),(\d+)\].*? (?:frame|mFrame)=\[(\d+),(\d+)\]\[(\d+),(\d+)\]',c,re.S)
 assert m, c[:4000]
 p=list(map(int,m.groups()[:4])); f=list(map(int,m.groups()[4:]));assert p[0]<f[0]<f[2]<p[2] and p[1]<=f[1]<f[3]<=p[3],(p,f)
 return p,f

sh(s,'appops','set','com.handy.voice','SYSTEM_ALERT_WINDOW','allow')
sh(s,'am','start-foreground-service','-n','com.handy.voice/.DictationService');time.sleep(2)
p,f=frame(s);time.sleep(2);assert frame(s)==(p,f), 'Idle overlay drift'
x=(f[0]+f[2])//2;y=(f[1]+f[3])//2
sh(s,'input','swipe',str(x),str(y),'2',str(y-20),'220');time.sleep(1)
frame(s);capture(s,'bubble-left')
for rotation in ('1','0'):
 sh(s,'settings','put','system','user_rotation',rotation);time.sleep(2)
 frame(s);capture(s,'bubble-rotation-'+rotation)
sh(s,'am','force-stop','com.handy.voice')
if api>=30: sh(s,'cmd','overlay','disable','--user','0','com.android.internal.display.cutout.emulation.waterfall')
print('ANDROID SAFE-AREA CHECKS PASSED: API',api)
