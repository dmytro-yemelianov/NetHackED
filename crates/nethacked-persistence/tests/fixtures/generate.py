"""Independent canonical vectors, Python stdlib only; never invokes Rust encoder.
Revision repair1. Seed is the archived pre-repair independent D1 vector.
Run from repository root with UV_CACHE_DIR=/private/tmp/uv-owner-wire uv run
--no-sync --python .venv/bin/python crates/nethacked-persistence/tests/fixtures/generate.py
"""
import copy, hashlib, json, pathlib, struct
root=pathlib.Path('crates/nethacked-persistence/tests/fixtures')
# The seed is copied verbatim to keep this generator reproducible outside /tmp.
s=json.loads((root/'seed-graph.json').read_bytes())
c=s['catalog']
def ref(kind,serial=1,mode='live'):return dict(target=dict(kind=kind,serial=serial),mode=mode)
def identity(name):return dict(name=name,version=1,artifact='01'*32)
def provider():return dict(module=ref('module'),artifact='02'*32,endpoint='query')
def authority():return dict(access='write',scope=dict(kind='keys',resources=[dict(owner_name='objects',key='z'),dict(owner_name='objects',key='a')]))
def permit():return dict(capability=dict(provider_name='module',endpoint='query',major=1),resolution=[provider()],effective=authority())
def binding(checkpoint=3):return dict(operation=ref('operation'),lease=ref('lease'),checkpoint=checkpoint,phase='apply',cleanup_identities=[3,9])
def stamp():return dict(owner=ref('owner'),module=provider(),generation=2,revision=3,schema_digest='01'*32)
def recorded():return dict(policy_name='record-policy',version=1,record=ref('input_record'),provider=provider(),request='03',binding=binding(),caller=ref('module'),caller_artifact='02'*32,permit=permit())
def observation_receipt(nested=False):return dict(stamp=stamp(),query='query',caller=ref('module'),caller_artifact='02'*32,permit=permit(),data_digest='03'*32,dependencies=[observation_receipt()] if nested else [],recorded_inputs=[recorded()])
def helper():return dict(provider=ref('module'),slot=2,ceiling=authority(),closure=[dict(permit=permit(),authority=authority())],code=ref('pin'))
def observation(nested=False):return dict(receipt=observation_receipt(nested),data='05',dependencies=[observation()] if nested else [],code=ref('pin'),behavior_revision=7,helper=helper(),execution=binding())
def participant(checkpoint=3):return dict(serial=1,stamp=stamp(),caller=ref('module'),caller_artifact='02'*32,binding=binding(checkpoint),command='prepare',permit=permit(),behavior=[provider()],next_digest='04'*32,event_count=1,request_digest='05'*32,recorded_inputs=[recorded()])
def declaration(checkpoint=3):return dict(caller=ref('module'),caller_artifact='02'*32,binding=binding(checkpoint),coordinator=permit(),provider=provider(),required_writes=[ref('owner')],effect_order=[ref('owner')],readonly=[observation(True)],retained_code=ref('pin'),policy=identity('policy'))
c['publication_domains']=[dict(issuing_host=17,high_water=5)]
for used in c['consumed_tickets']:used['issuing_host']=17
for candidate in c['candidates']:
 candidate.update(receipt=participant(),prepared=dict(dependencies=[observation(True)],code=[ref('pin')],helper=helper(),recorded_inputs=[recorded()]))
for batch in c['batches']:batch.update(issuing_host=17,declaration=declaration())
for publication in c['publications']:
 publication.update(batch=None,declaration=declaration(1),accounting=identity('accounting'),accounting_payload='00',receipt=dict(issuing_host=17,ticket=2,epoch=9,caller=ref('module'),caller_artifact='02'*32,binding=binding(1),coordinator=permit(),policy=identity('policy'),participants=[participant(1)],observations=[observation_receipt(True)],events=['00'],reply='0600',history_encoding='01'))
for workflow in c['workflows']:
 workflow.update(phase='apply',checkpoint=3,pipeline_digest='06'*32,ticket=dict(operation=ref('operation'),sequence=3,lease=ref('lease'),pipeline_digest='06'*32,phase='previous'))
def freeze(name,value):
 body=json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()
 (root/(name+'.json')).write_bytes(body)
 (root/(name+'.wire')).write_bytes(b'NHOWNR01'+struct.pack('<IQ',1,len(body))+hashlib.sha256(body).digest()+body)
freeze('graph',s)
empty=copy.deepcopy(s);empty['catalog']={k:[] for k in c};freeze('empty',empty)
# Independent complete positive variants. Ticket is unrelated to workflow sequence.
host=copy.deepcopy(s);host['catalog']['workflows']=[];host['catalog']['operations'][0]['sequence']=0;freeze('host-ticket',host)
initial=copy.deepcopy(s);ic=initial['catalog']
for key in ['batches','candidates','publications','consumed_tickets']:ic[key]=[]
ic['operations'][0].update(sequence=0,checkpoint=0)
iw=ic['workflows'][0];iw.update(status='running',sequence=0,checkpoint=0,ticket=None,phase='initial',trace=[],accepted_publications=[],accepted_inputs=[],cleanup=[])
freeze('initial',initial)
# Two live owners: sorted required writes and reverse effect order are independent.
multi=copy.deepcopy(s);mc=multi['catalog']
mc['tombstones'][0]['id']['serial']=3;mc['retired'][0]['serial']=3
mc['callbacks'][0]['source']['target']['serial']=3
second=copy.deepcopy(mc['owners'][0]);second['id']['serial']=2
mc['owners'].append(second)
second_dispatch=copy.deepcopy(mc['current_dispatch'][0]);second_dispatch['owner']['target']['serial']=2
mc['current_dispatch'].append(second_dispatch)
second_candidate=copy.deepcopy(mc['candidates'][0]);second_candidate['id']['serial']=2
second_candidate['owner']=ref('owner',2);second_candidate['stamp']['owner']=ref('owner',2)
second_candidate['receipt']['serial']=2;second_candidate['receipt']['stamp']['owner']=ref('owner',2)
mc['candidates'].append(second_candidate)
mc['batches'][0]['candidates'].append(ref('candidate',2))
for decl in [mc['batches'][0]['declaration'],mc['publications'][0]['declaration']]:
 decl['required_writes']=[ref('owner'),ref('owner',2)]
 decl['effect_order']=[ref('owner',2),ref('owner')]
second_participant=participant(1);second_participant['serial']=2;second_participant['stamp']['owner']=ref('owner',2)
mc['publications'][0]['receipt']['participants'].insert(0,second_participant)
for counter in mc['counters']:
 if counter['kind']=='owner':counter['high_water']=3
 if counter['kind']=='candidate':counter['high_water']=2
freeze('multi-owner',multi)
# Archive generator/input/output provenance without timestamps or Rust dependencies.
files=['generate.py','seed-graph.json']+[n+e for n in ['empty','graph','host-ticket','initial','multi-owner'] for e in ['.json','.wire']]
(root/'provenance.json').write_text(json.dumps(dict(revision='repair1',seed_origin='writer independent graph, archived snapshot-before',sha256={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files}),indent=2)+'\n')
