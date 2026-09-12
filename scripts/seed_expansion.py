#!/usr/bin/env python3
"""One-off seed: expand showcase packs to full-size dictionaries.

Writes reproducible wordlist sources to packs/sources/ (txt/csv), then
drives scripts/build_pack.py (build for words_en + medical, expand-pack
for the rest) so the ADDING.md flow is exactly what produced these packs.

Offline-safe: English tail comes from /usr/share/dict/web2 (macOS system
wordlist); everything else is curated in this file. No downloads.

  python3 scripts/seed_expansion.py
"""

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BP = [sys.executable, str(ROOT / "scripts" / "build_pack.py")]
SRC = ROOT / "packs" / "sources"
SRC.mkdir(exist_ok=True)

EN_CORE = """the 1000000
be 800000
to 750000
of 700000
and 650000
a 600000
in 550000
that 400000
have 350000
hello 9000
world 8000
help 7000
hell 500
fun 6000
good 20000
home 15000
house 12000
time 25000
people 22000
water 14000
food 13000
day 24000
night 11000
love 18000
work 21000
school 16000
friend 9000
family 10000
happy 8000
sad 4000
big 15000
small 9000
new 30000
old 12000
first 14000
last 13000
long 11000
great 17000
little 10000
own 19000
other 23000
many 16000
some 26000
what 28000
when 20000
where 12000
which 18000
there 27000
their 25000
about 22000"""

EN_COMMON = """able account across action active address admit adult after again
against age ago agree ahead alarm album alert allow almost alone along aloud
already also always amazing among amount anger angle angry animal answer apple
apply area argue arm arrive art article artist asleep attack attend author
auto avoid awake award aware baby back badge bag bake balance ball bank bar
base basic basis bath beach beast beauty become bed begin behind being believe
bell belong below belt bench best better between beyond bicycle big bike bill
bird birth black blade blame blank blend blind block blood bloom blow blue
board boat body boil bold book boom boost booth bottle bound bowl box brain
brave bread break breath breeze brick bridge brief bright bring broad broke
brown brush build bunch burn burst bus busy buy buyer cable cake call calm
camera camp cancel candy cap card care carry catch cause chain chair chalk
change charge charm chart chase cheap check cheese chest chicken chief child
choice choose chronic church circle city civil claim class clean clear clerk
click cliff climb clock close cloth cloud coach coast coat code coffee coin
cold collar color comb come comfort comic common copper copy corner cost
could count couple course court cover crack craft crash crazy cream credit
crime cross crowd crown crude crush cry daily dairy dance data date dawn deal
dear death debate debt decide deep deer delay deliver demand dense depth
derive describe desert design detail device dial diary digit dinner direct
dirty disco ditch dive dizzy doctor document dog dollar door double doubt
doubt draft drain drama dream dress drink drive drop dry due during dust
eager eagle early earn earth easily east easy eat edge eight elbow elder elect
empty enable end enemy energy engine enjoy enough enter entire entry equal
error essay event every exact exam example excess exchange excite exist extra
eye face fact factor fail faint fair faith fall false fancy far farm fashion
fast fatal father fault favor feast fence fever field fifth fifty fight final
finance find finger finish fire firm first fish flame flash flat flavor fleet
flesh float floor flour flower fluid focus force forest forget form forth
forty forum found frame fresh friday friend front frost fruit fuel fully funny
giant gift girl give glad glass glove glow glue goal goat goes going gold
grace grade grain grand grant grape graph grass grave great green greet grief
grill grind grocer group Grove guard guess guest guide habit hair half hall
hand handle handy happen harbor hard harsh harvest hate heart heat heavy hello
helmet hence honey honor horse hospital hotel house human humid humor hurry
husband ideal image imagine inbox index inner input issue ivory jewel join
joint joke judge juice july june juice keen keep keyboard kindly king kiss
kitchen knife knock known label labor large laser late laugh layer learn
least leave legal lemon level light limit listen live liver local lock lodge
logic loose lord lorry lunch magic major maker manage march market marry
match mate matter mayor maybe meal mean meant medal media meet melody melt
member merry metal meter method middle might minor minus minute mirror miss
mistake mix model modem money month moral motor mount mouse mouth movie music
naive naked nasty naval near neat need nerve never newly night noble noise
north novel nurse ocean offer office often older olive onion open opera order
organ other ought ounce outer owner paint panel paper party pass patch pause
peace peach pedal penny phase phone photo piano piece pilot pitch pizza place
plain plane plant plate plaza point pound power press price pride prime print
prior prize proof proud prove pulse punch pupil purchase purple purse push
queen query quest queue quick quiet quite quota quote radar radio raise rally
range rapid ratio reach react ready realm rebel receive recipe record refer
refresh renew rent repair repeat reply report rescue rest result return reveal
review rider right river roast robot rock role roof room root rope rough round
route royal rugby ruler rumor rural salad salt same sand scale scene score
screen script search season seat second sense serve seven shade shake shall
shame shape share sharp sheep sheet shelf shell shift shine shirt shock shoe
shoot shop shore short shout shown sight silly since sixth sixty skill skin
skirt sleep slice slide slope small smart smell smile smoke snack snake solar
solid solve sorry sound south space spare speak speed spell spend spice split
spoke sport staff stage stair stand start state station steam steel steep steer
stick still stock stone stood store storm story stove strand strange strap
straw streak stream street stress stretch strict strike string strip style
sugar sunny sunset super sweet swift swing sword table taken taste teach teeth
thank their theme there thick thing think third thirty those three threw
throw thumb ticket tiger tight tired title today token tooth topic total touch
tough tower town trace track trade trail train travel treat trend trial tribe
trick truck truly trust truth twice twist uncle under union unite until upper
upset urban usage usual valid value video visit vital voice vote voter waste
watch water wealth wear weave wednesday weed week weigh weird welcome wheat
wheel where which while white whole whose woman women wonder wood word would
wound wrist write writer wrong wrote yard year yeast yellow young youth zero
zebra plane train ship truck motor engine wheel brake light mirror seat belt
ticket trip hotel beach photo music song dance story game play match score
team coach player field court ball goal net win lose draw point league cup
tennis golf swim run jump throw catch kick punch box ring bell round judge
proud brave loyal honest kind polite rude mean cruel fair unfair just law rule
court judge jury trial proof truth lie honest cheat steal rob crime jail cell
guard alarm lock key door wall roof floor stair lift lobby room suite bed bath
towel soap brush comb cream lotion pill syrup dose nurse ward clinic cure care
apple bread rice pasta cheese egg bacon toast jam honey milk juice water tea
lunch dinner snack fruit grape lemon mango melon berry nut bean grain wheat
oats honey sugar salt spice herb onion garlic ginger chili curry soup stew grill
roast fry bake boil steam peel chop slice dice grate mash whip pour serve table
spoon fork knife plate bowl cup glass mug tray cloth napkin menu bill tip chef
cook baker waiter guest host party invite gift card wrap ribbon cake candle
balloon toast cheer merry happy glad joy smile laugh grin giggle humor joke
prank fool witty clever smart wise dull silly naive brave bold shy timid proud
calm tense relax rest sleep dream nap yawn snore alarm clock early late noon
midnight dawn dusk sunset sunrise today tomorrow yesterday week month year
monday tuesday thursday saturday sunday season spring summer autumn winter
weather storm rain snow hail fog mist cloud sky sun moon star planet comet
ocean sea lake river pond pool wave tide sand shell coral reef island coast
stone rock hill vale peak cliff cave den hole pit ditch ridge plain field
meadow farm crop harvest plow sow reap barn silo fence gate path road lane
street bridge tunnel cross sign signal lamp post curb curb curb mailbox porch
lawn hedge bloom weed rake shovel hose sprinkler mower trim edge mulch soil
clay sand gravel pebble boulder marble slate chalk lime rust steel iron brass
copper nickel zinc tin lead glass glaze gloss paint dye stain tint shade tone
color black white gray brown red pink orange beige ivory olive lime mint teal
navy beige khaki denim suede wool cotton silk linen lace denim boot shoe heel
sneaker sandal slipper sock glove scarf hat cap hood cloak gown suit dress
skirt blouse shirt pants jeans shorts coat jacket vest sweater hoodie belt tie
button zip pocket collar cuff sleeve hem seam patch badge pin clip clasp chain
purse wallet bag pack case trunk chest drawer shelf rack stand podium stage
curtain blind shade shutter pane sill porch deck patio fence rail stair step
ladder ramp dock pier wharf buoy anchor sail mast deck hull keel oar paddle
river rapid fall spring well pump pipe drain sewer gutter spout faucet tap sink
tub shower bath steam sauna towel robe slipper bench locker gym mat towel
scale clock timer watch alarm dial hand tick second minute hour day week month
season year decade score dozen pair couple few many much more most less least
every each either neither both all any some none one two three four five six
seven eight nine ten twenty thirty forty fifty hundred thousand dozen score
computer message walk internet email mobile laptop printer audio download
upload login password username website browser app folder scroll swipe battery
charger wifi backup update install delete save send forward attach spam contact
channel follow text vibrate software hardware server client cloud sync async
pixel cursor icon menu tab window frame panel dialog toast badge notch home
button slider switch toggle theme font bold italic underline strike highlight
copy paste cut undo redo find filter sort pin archive mute snooze remind snooze
calendar event invite meet call conference camera mic speaker volume bright dim
flash focus zoom crop trim merge split join export import render stream live
story reel short viral trend tag topic thread poll quiz survey vote rate rank
badge score level quest badge streak reward coupon deal cart checkout pay ship
track return refund swap trade gift card code promo stock shelf aisle fresh
frozen chill microwave oven stove kettle pot pan dish tray bake roast toast
cereal salad fruit veggie snack dessert candy cookie cake pie tart tart crust
dough crust crumb bite chew sip gulp burp yum tasty bland spicy sour bitter
sweet salty savory aroma flavor feast famine hunger thirst full empty plate
picnic brunch buffet feast roast grill smoke cure pickle jam jelly syrup sauce
gravy broth stock roux marinade glaze crust sear char broil poach steam braise
stew simmer boil bubble froth foam fizz pop crackle crunch chew munch nibble
nurse doctor patient clinic pill shot dose drug cure heal mend scar bruise ache
cough sneeze fever chill sweat dizzy faint weak tired sore stiff numb tingle
itch rash bump lump mole wart blister burn cut scrape scratch bruise sprain
bandage cast sling crutch brace splint balm lotion cream gel foam spray drop
syrup tablet caplet capsule vax jab booster checkup exam test lab scan xray
chart file record history symptom sign ache pain throb sting burn itch tingle
breathe inhale exhale cough wheeze gasp sigh yawn hiccup burp blink wink stare
glance peek peer gaze gawk grin smirk frown pout scowl glare wink nod shake
shrug wave point beckon summon call hail flag stop go wait stay sit stand kneel
bow bend stretch reach grab hold grip clutch grasp seize snatch steal borrow
lend give take fetch carry haul drag push pull lift lower raise drop toss throw
catch kick punt boot stomp tramp march jog sprint dash dart rush hurry hasten
linger loiter wander roam rove stray drift float glide slide skid slip trip
stumble fall tumble roll spin twirl whirl swirl twist turn bend fold curl coil
loop knot tie bind wrap pack load unload stack pile heap mound hill dune ridge
valley gorge canyon bluff mesa butte Tor peak crest crown cap top tip apex
base foot baker chef cook meal dish course feast snack bite morsel crumb drop
grain loaf roll bun biscuit cracker chip crisp crunch wafer cone cup scoop
shake float sundae split banana apple pear plum prune date fig kiwi lime lemon
melon berry cherry grape fruit juice cider punch shake malt float freeze chill
frost ice steam smoke vapor mist fog dew frost hail sleet snow slush rain pour
drizzle sprinkle shower storm tempest gale gust breeze draft chill nip bite
sting burn sear scorch singe char ash ember flame blaze flare flash glint glow
gleam glimmer shimmer shine glare beam ray shaft streak beam glow dusk dawn
noon midnight morning evening night today tonight tomorrow yesterday morrow
epoch era age eon time tense past present future soon later never ever always
often seldom rarely daily nightly weekly yearly hourly soon anon later after
before since until while during amid among between through across along past
over under above below beneath behind beside near far nigh afar yonder here
there where everywhere nowhere somewhere anywhere elsewhere home away out in
up down left right forth back forth thence hence hither yon beyond above"""

MEDICAL = """pain fever cough cold flu ache chill sweat fatigue nausea vomit
diarrhea constipation reflux ulcer cramp spasm bleed blister rash itch hives
swelling edema bruise bump lump cyst polyp wart mole abscess boil sore wound
cut burn sting bite fracture sprain strain tear dislocation hernia prolapse
arthritis gout bursitis sciatica backache headache migraine dizzy faint seizure
stroke shock coma allergy sneeze wheeze asthma bronchitis pneumonia sinusitis
otitis tonsillitis laryngitis gastritis colitis hepatitis nephritis cystitis
urethritis dermatitis cellulitis meningitis sepsis infection virus bacteria
fungus parasite germ pus mucus phlegm bile stool urine blood clot thrombus
embolus hemorrhage aneurysm varicose gangrene necrosis tumor benign malignant
cancer carcinoma sarcoma lymphoma leukemia melanoma metastasis biopsy lesion
diabetes anemia hypertension hypotension arrhythmia murmur infarct ischemia
asthma eczema psoriasis acne dandruff wart impetigo shingles measles mumps
rubella chickenpox smallpox polio rabies tetanus cholera typhoid malaria
dengue tuberculosis tb pneumonia influenza covid cold sore ulcer sore throat
earache toothache bellyache cramp colic appendicitis gallstone kidney stone
bladder cyst ovary fibroid endometriosis prostatitis vaginitis thrush yeast
herpes wart hiv aids syphilis gonorrhea chlamydia scabies lice ringworm
athlete foot wart corn callus bunion ingrown hangnail whitlow felon sty
conjunctivitis pinkeye cataract glaucoma myopia hyperopia astigmatism deaf
tinnitus vertigo motion sick dizziness numbness tingling tremor twitch tic
palsy paralysis paresis numb stroke mini tia aneurysm bleed clot dementia
alzheimer parkinson epilepsy autism adhd anxiety panic phobia depression
bipolar schizophrenia insomnia apnea snore narcolepsy sleepwalk nightmare
obesity anorexia bulimia binge thirst hunger dizzy weak faint pale flush
sweat chill shiver goosebump hiccup burp fart belch heartburn indigestion
bloat gas ulcer sore canker cold flu ache strain pull sore stiff sore neck
wry stiff frozen shoulder tennis elbow carpal tunnel trigger finger shin
splint runner knee jumper housemaid knee baker cyst heel spur plantar arch
flat bunion hammer toe gout toe turf ingrown corn callus blister chafe rub
sunburn frostbite windburn chapped cracked split peel flake scale crust scab
scar keloid stretch mark birthmark freckle mole beauty liver spot age wart
skin tag cherry hemangioma spider vein varicose thread birth defect
heart lung liver kidney brain bone muscle nerve skin eye ear nose throat
artery vein cell tissue organ gland spine rib skull pelvis femur tibia humerus
vertebra joint tendon ligament cartilage marrow plasma serum neuron synapse
cortex cerebellum hippocampus pituitary thyroid adrenal pancreas spleen
gallbladder bladder ureter urethra prostate ovary uterus cervix placenta
testis diaphragm trachea bronchus alveolus esophagus stomach intestine colon
rectum appendix aorta lymph retina cornea iris pupil lens cochlea eardrum
sinus tonsil molar incisor gum palate jaw cheek forehead chin neck shoulder
elbow wrist hip knee ankle heel toe finger thumb nail hair follicle duct
vessel capillary valve atrium ventricle septum sternum clavicle scapula
patella meniscus bursa abdomen thorax groin armpit navel temple nostril
diaphragm bowel colon duodenum jejunum ileum cecum spleen thymus marrow
aspirin ibuprofen paracetamol penicillin amoxicillin azithromycin
ciprofloxacin doxycycline metronidazole insulin metformin statin warfarin
heparin morphine codeine tramadol lidocaine ketamine diazepam lorazepam
sertraline fluoxetine omeprazole prednisone salbutamol cetirizine loratadine
amlodipine lisinopril losartan atenolol metoprolol furosemide digoxin vaccine
antacid laxative diuretic syrup tablet capsule drops ointment cream dose
dosage diagnosis prognosis symptom sign syndrome chronic acute severe mild
moderate clinic hospital ward icu nurse doctor surgeon physician pharmacist
dentist midwife sterile surgery xray mri scan ultrasound ecg test culture
swab injection infusion transfusion dialysis anesthesia suture bandage cast
splint stent graft bypass endoscopy therapy rehab exam checkup screening
dose remedy cure heal recover relapse remit flare bout attack spell episode
triage referral consult second opinion chart record history family travel
allergy list medication reconciliation dose frequency route side effect
overdose withdrawal tolerance dependence addiction rehab sober clean relapse
poison venom sting antivenom antidote charcoal lavage pump stomach"""

# Nepali "devanagari:roman" pairs (romanization is a lookup hint only).
NEPALI_NEW = """एक:ek दुई:dui तीन:tin चार:char पाँच:panch छ:chha सात:saat आठ:aath
नौ:nau दश:dash सय:saya हजार:hajar लाख:lakh आज:aaja भोलि:bholi हिजो:hijo
बिहान:bihan बेलुका:beluka दिउँसो:diuso समय:samaya बर्ष:barsha महिना:mahina
हप्ता:hapta घडी:ghadi आमा:aama बुबा:buba दाइ:dai भाइ:bhai दिदी:didi
बहिनी:bahini छोरा:chhora छोरी:chhori बच्चा:bachcha महिला:mahila पुरुष:purush
गाउँ:gaun शहर:shahar बजार:bazar पसल:pasal विद्यालय:bidhyalaya कलेज:college
अस्पताल:aspatal मन्दिर:mandir चोक:chok पुल:pul पहाड:pahad हिमाल:himal
नदी:nadi खोला:khola ताल:taal जंगल:jungle खेत:khet बारी:bari भात:bhat
दाल:daal तरकारी:tarkari रोटी:roti अचार:achar मासु:masu माछा:machha अण्डा:anda
दूध:dudh दही:dahi चिया:chiya चिनी:chini नुन:nun तेल:tel चामल:chamal मकै:makai
गहुँ:gahun आलु:aalu प्याज:pyaj लसुन:lasun अदुवा:aduwa केरा:kera स्याउ:syau
सुन्तला:suntala आँप:aanp टाउको:tauko आँखा:aankha कान:kaan नाक:naak मुख:mukh
दाँत:daant घाँटी:ghaanti कपाल:kapaal हात:haat खुट्टा:khutta औंला:aunla
छाला:chhala रगत:ragat हड्डी:haddi मुटु:mutu फोक्सो:phokso पेट:pet ढाड:dhad
कुकुर:kukur बिरालो:biralo गाई:gai भैंसी:bhainsi बाख्रा:bakhra भेडा:bheda
घोडा:ghoda हात्ती:hatti बाँदर:baandar सर्प:sarpa चरा:chara पुतली:putali
मौरी:mauri फूल:phool रूख:rookh पात:paat फल:phal बीउ:biu घाम:gham हावा:hawa
बादल:baadal हिउँ:hiun गर्मी:garmi जाडो:jado वर्षा:barsha आकाश:aakash
तारा:tara जुन:jun सूर्य:surya जानु:jaanu आउनु:aaunu खानु:khaanu पिउनु:piunu
बस्नु:basnu उठ्नु:uthnu सुत्नु:sutnu हिँड्नु:hidnu बोल्नु:bolnu सुन्नु:sunnu
हेर्नु:hernu लेख्नु:lekhnu पढ्नु:padhnu गर्नु:garnu दिनु:dinu लिनु:linu
किन्नु:kinnu बेच्नु:bechnu खेल्नु:khelnu हाँस्नु:haasnu रुनु:runu सिक्नु:siknu
बुझ्नु:bujjhnu सोच्नु:sochnu खोज्नु:khojnu भेट्नु:bhetnu राख्नु:rakhnu
पठाउनु:pathaunu खोल्नु:kholnu ठिक:thik सजिलो:sajilo गाह्रो:gahro छिटो:chhito
धेरै:dherai थोरै:thorai सबै:sabai केही:kehi यस्तो:hyasto त्यस्तो:tyasto
कस्तो:kasto सफा:sapha फोहोर:fohor तातो:tato चिसो:chiso काम:kaam शान्ति:shanti
दुःख:dukha सुख:sukha जीवन:jiwan सपना:sapana आशा:aasha धर्म:dharma
संस्कृति:sanskriti गीत:geet नाच:naach फिल्म:film फोटो:photo मोबाइल:mobile
फोन:phone खबर:khabar सरकार:sarkar नेता:neta चुनाव:chunab कानुन:kanun
डाक्टर:doctor औषधि:aushadhi बिरामी:birami स्वास्थ्य:swasthya धन्यबाद:danyabad
किन:kina कहाँ:kahan कहिले:kahile को:ko के:ke कसरी:kasari किनभने:kinabhane
अनि:ani तर:tara कि:ki पनि:pani मात्र:matra न:na छैन:chhaina हो:ho हुन्छ:hunchha
गयो:gayo आयो:aayo खायो:khayo राम्ररी:ramrari बिस्तारै:bistarai फेरि:pheri
अहिले:ahile पछि:pachhi अघि:aghi माथि:mathi तल:tala भित्र:bhitra बाहिर:bahira"""

JS_NEW = """console log document window alert prompt confirm promise resolve
reject map filter reduce push pop shift slice splice split join replace
includes keys values entries assign super get set delete void debugger eval
fetch then error json math date set symbol bigint proxy reflect module require
process buffer target append remove children parent generator iterator cookie
history location navigator screen arrow callback closure scope prototype
constructor hoist instanceof parseInt parseFloat stringify regexp weakset
weakmap localStorage querySelector innerHTML className preventDefault array
object string number boolean function method property event listener state
props render component effect ref memo context router route link nav form
input button select option table row cell list item menu dialog modal toast
theme dark light color size width height margin padding border radius shadow
flex grid center top bottom left right front back next prev first last index
count total sum avg min max sort find every some flat concat reverse fill
if else for while return in do case default true false null undefined as of
is static private public protected accessor declare asserts satisfies using
out any globalThis isNaN isFinite encodeURI decodeURI encodeURIComponent
decodeURIComponent parse apply call bind has add clear test exec flags
ignoreCase sticky message stack cause all allSettled race withResolvers
groupBy fromEntries fromAsync toSorted toReversed toSpliced findLast at with
padStart padEnd trimStart trimEnd repeat startsWith endsWith matchAll
replaceAll match search toLowerCase toUpperCase substring charAt normalize
create freeze seal defineProperty getPrototypeOf hasOwn random floor ceil
round trunc hypot sign cbrt pow abs sqrt done value length name arguments
AggregateError TypeError RangeError SyntaxError ReferenceError assert enum
type namespace implements abstract override readonly unknown never infer keyof
currentTarget bubbles composed click change load resize keydown keyup
textContent outerHTML innerText closest matches toggle prepend shadowRoot
attachShadow getRootNode isConnected insertAdjacentHTML scroll scrollTo focus
blur submit on off play pause currentTime duration volume loop setTimeout
setInterval clearTimeout clearInterval requestAnimationFrame cancelAnimationFrame
observe disconnect observer sessionStorage indexedDB performance structuredClone
queueMicrotask AbortController Headers Request Response WebSocket Worker
clipboard geolocation status statusText url ok body text arrayBuffer blob clone
getElementById querySelectorAll createElement appendChild removeChild
insertBefore cloneNode setAttribute getAttribute classList dataset style
exports dirname filename argv env stdout stdin stderr exit cwd mkdir
appendFile readFile writeFile readdir stat pipe emit describe it expect
beforeEach afterEach beforeAll afterAll mock signal"""

RUST_NEW = """box rc arc option some none result ok err vec string str slice array
tuple hashmap hashset iter collect clone copy debug display default drop send
sync borrow owned lifetime generic macro println print format todo panic
assert derive repr allow deny warn inline cfg test bench super self in union
offset transmute forget pin future stream thread spawn join main char bool
usize isize u8 u16 u32 u64 i8 i16 i32 i64 f32 f64 cell refcell mutex atomic
channel error kind from into try map unwrap expect
lock read write
eprintln assert_eq assert_ne debug_assert unimplemented unreachable matches
include include_str include_bytes option_env compile_error stringify file line
column module_path concat write writeln format_args thread_local link
export_name no_mangle deprecated forbid must_use track_caller cold doc
as new leak into_raw from_raw downgrade upgrade weak as_ref as_mut borrow_mut
try_from try_into and_then or_else map_or unwrap_or unwrap_or_else is_some
is_none is_ok is_err as_slice to_vec to_string to_owned into_iter iter_mut
enumerate filter_map flat_map find_map find any all sum product fold for_each
count last nth next peekable chain take skip zip by_ref clamp pow sqrt abs
hypot push pop insert remove retain split trim starts_with ends_with contains
replace lines chars bytes is_empty capacity reserve clear drain extend append
windows chunks sort reverse binary_search partition try_fold successors
from_iter from_fn once empty repeat stdin stdout stderr args current_dir
create open metadata exists remove_file create_dir read_dir read_to_string
read_line flush seek path pathbuf bufreader bufwriter oncelock lazylock cow
manuallydrop maybeuninit phantomdata wrapping saturating nonzero range bound
ordering hash hasher deref closure abstract become do final macro override
priv typeof unsized virtual yield gen"""

# Placeholder-grade tokens seeded earlier (`*2`/`*m` dedupe hacks, bare
# fragments): removed from the built packs by main() below, replaced by the
# real std names above. Never reintroduce suffixed variants.
RUST_DROP = """array2 asmut asref borrowm boxm errm okm lifet-param param arg
ret val num idx cap ptr strm slice2 vecm scop sendm syncm pinm own guard
cause context"""

HTML_NEW = """aside figure figcaption details summary dialog canvas video audio
source track embed iframe object picture svg path circle template slot code
pre hr br strong em small mark del ins sub sup kbd samp abbr cite q time
progress meter output fieldset legend class id href src alt action method name
value checked disabled readonly required rows cols target rel media content
defer placeholder
charset viewport maxlength colspan rowspan contenteditable draggable hidden
tabindex role aria label described labelledby controls autoplay loop muted
poster preload kind srclang open autofocus multiple min
max step pattern list spellcheck translate accesskey inert popover
address area b base bdi bdo big blockquote caption center col colgroup data
datalist dd dfn dl dt font h1 h2 h3 h4 h5 h6 i map marquee menu noscript
optgroup s search tbody tfoot thead u var wbr dir part accept accept-charset
allow allowfullscreen async capture coords crossorigin datetime decoding
default dirname download enctype for form formaction headers height hreflang
http-equiv integrity is ismap itemid itemprop itemref itemscope itemtype lang
loading low high minlength novalidate optimum ping popoveraction popovertarget
referrerpolicy sandbox scope selected shape sizes srcset start width wrap
enterkeyhint inputmode autocomplete autocapitalize fetchpriority playsinline
controlslist onclick onchange onsubmit onload oninput onkeydown onkeyup
onfocus onblur"""

# Same placeholder cleanup as Rust: `*2`-suffixed tag/attr stand-ins seeded
# earlier are removed from the built pack by main(), replaced by the real
# tags/attrs above.
HTML_DROP = """span2 div2 header2 footer2 nav2 main2 section2 article2 default2
disabled2 dialog2"""

# (emoji, keyword) — seq is T9(keyword); loader ignores `key`.
EMOJI_NEW = """😀 grin 😉 wink 😎 cool 😍 adore 😘 kiss 🤗 hug 🤔 think 🥳 party
😴 sleep 😵 dizzy 🤓 nerd 🤡 clown 👻 ghost 🤖 robot 💩 poop 🙈 monkey 🦁 lion
🐯 tiger 🐻 bear 🐼 panda 🐸 frog 🐷 pig 🐮 cow 🐦 bird 🐟 fish 🐳 whale 🐍 snake
🦋 butterfly 🐝 bee 🌸 bloom 🌹 rose 🌳 tree 🌵 cactus 🌕 moon ✨ sparkle ☁️ cloud
🌧 rain ❄️ snow 🌈 rainbow 🌊 wave 🌍 earth 🚗 car 🚲 bike ✈️ plane 🚂 train
🚢 ship 🏠 house 📱 phone 💻 laptop 📷 camera 📚 books 🎁 gift 🎂 cake 🍵 tea
🍺 beer 🍎 apple 🍌 banana 🍇 grapes 💯 score ❗ alert 💡 bulb 🔑 keys 🔒 lock
🔔 bell 🎸 guitar ⚽ ball 🏀 hoops 🎮 play 🏆 win 👶 baby 🙏 pray 💪 strong 👋 hi
👌 okay ✌️ peace 👀 look 👂 hear 👄 lips 🦶 foot 💔 heartbreak 💤 rest 🎧 tunes
📝 memo 📌 pin 🧹 broom 🧲 magnet ⏰ alarm 🎈 balloon 🪁 kite 🧸 teddy 🪆 doll
😭 sob 😡 rage 😤 huff 😇 angel 🤠 cowboy 🥸 disguise 😷 mask 🤒 sick 🤕 hurt
🥶 cold 🥵 hot 🤯 explode 🤩 wow 🥱 yawn 😪 sleepy 🤤 drool 😛 tongue 😜 wacky
🤪 zany 🤑 rich 🤐 zip 🫡 salute 🫠 melt 🫣 peek 💗 grow 💓 beat 💞 revolve
💘 cupid 💝 wrap 🧡 orange 💛 yellow 💚 green 💙 blue 💜 purple 🖤 black
🤍 white 🤎 brown 🩷 pink 👎 dislike 🙌 cheer 🤝 shake ✊ fist ✋ palm 👐 open
🤲 beg 👨 man 👩 woman 👦 boy 👧 girl 👴 grandpa 👵 granny 👮 cop 🎅 santa
🐭 mouse 🐰 bunny 🦊 fox 🐨 koala 🐧 penguin 🦆 duck 🦅 eagle 🦉 owl 🦄 unicorn
🐢 turtle 🐙 octopus 🐬 dolphin 🐆 leopard 🦒 giraffe 🦓 zebra 🐘 elephant
🍏 sour 🍐 pear 🍊 tangerine 🍋 lemon 🍉 melon 🍓 berry 🫐 blueberry 🍒 cherry
🍑 peach 🥭 mango 🍍 pineapple 🥥 coconut 🥝 kiwi 🍅 tomato 🫒 olive 🥦 broccoli
🥒 cucumber 🌶 spicy 🌽 corn 🥕 carrot 🎱 pool 🏐 volley 🎾 tennis ⚾ baseball
🛹 skate 🏄 surf 🚕 taxi 🚌 bus 🚚 truck ➕ plus ♾️ forever"""

# Key fixups for rows already shipped with placeholder-grade keys.
EMOJI_KEY_FIX = {"🧸": "teddy"}

MATH_NEW = """\\phi phi \\psi psi \\chi chi \\eta eta \\mu mu \\nu nu \\xi xi
\\zeta zeta \\tau tau \\kappa kappa \\rho rho \\partial part \\nabla nabla
\\forall all \\exists exi \\in in \\subset subset \\supset superset
\\cup cup \\cap cap \\wedge wedge \\vee vee \\frac frac \\log log \\ln ln
\\exp exp \\sin sin \\cos cos \\tan tan \\lim limit \\to to
\\hbar hbar \\ell ell \\angle angle \\perp perp \\parallel par \\cong congr
\\equiv equiv \\sim similar \\propto prop \\pm plusmin \\cdot dot \\div div
\\circ circ \\ldots dots \\prime prime \\binom cho \\cbrt cube
\\upsilon upsilon \\varphi varphi \\vartheta vartheta \\varpi varpi
\\varrho varrho \\varepsilon varepsilon \\oint oint \\bigcup bigcup
\\bigcap bigcap \\bigoplus bigoplus \\bigotimes bigotimes \\coprod coprod
\\ll ll \\gg gg \\prec prec \\succ succ \\preceq preceq \\succeq succeq
\\simeq simeq \\asymp asymp \\doteq doteq \\models models \\vdash vdash
\\mid mid \\nmid nmid \\subseteq subseteq \\supseteq supseteq \\notin notin
\\ni ni \\rightarrow rightarrow \\leftarrow leftarrow \\Rightarrow Rightarrow
\\Leftarrow Leftarrow \\leftrightarrow leftrightarrow
\\Leftrightarrow Leftrightarrow \\mapsto mapsto \\implies implies \\iff iff
\\uparrow uparrow \\downarrow downarrow \\sec sec \\csc csc \\cot cot
\\min min \\max max \\sup supremum \\inf infimum \\det det \\gcd gcd
\\ker ker \\arg arg \\bmod bmod \\pmod pmod \\langle langle \\rangle rangle
\\lceil lceil \\rceil rceil \\lfloor lfloor \\rfloor rfloor \\vert vert
\\Vert Vert \\cdots cdots \\vdots vdots \\ddots ddots \\hat hat \\bar bar
\\tilde tilde \\vec vec \\dot dot \\ddot ddot \\overline overline
\\underline underline \\aleph aleph \\emptyset emptyset \\varnothing varnothing
\\bullet bullet \\star star \\dagger dagger \\ddagger ddagger \\oplus oplus
\\ominus ominus \\otimes otimes \\odot odot \\diamond diamond \\neg neg
\\top top \\bot bot \\mathbf mathbf \\mathit mathit \\mathcal mathcal
\\mathbb mathbb \\mathsf mathsf \\mathtt mathtt \\mbox mbox \\text text
\\begin begin \\end end \\item item \\quad quad \\qquad qquad
\\checkmark checkmark \\triangle triangle \\square square
\\blacksquare blacksquare \\left left \\right right \\middle middle
\\big big \\surd surd \\flat flat \\natural natural \\sharp sharp"""

# Non-standard commands seeded earlier: dropped from the built pack by
# main() (`\\sqrt2` is not a LaTeX command; `\\cbrt` is package-only).
# `\\sqrt` itself stays.
MATH_DROP = """\\sqrt2 \\cbrt"""

TITLES = {
    "words": "English Base", "medical": "Medical Terms", "ne": "Nepali Base (Romanized)",
    "js": "JavaScript Keywords", "rust": "Rust Keywords", "html": "HTML Tags",
    "emoji": "Emoji", "math": "Math/LaTeX",
}


def write_sources():
    # words_en: curated head + web2 tail
    core_words = [ln.split()[0] for ln in EN_CORE.splitlines()]
    have = set(core_words)
    common = []
    for w in EN_COMMON.split():
        w = w.strip().lower()
        if w and w not in have and len(w) <= 12:
            have.add(w)
            common.append(w)
    tail = []
    try:
        web2 = Path("/usr/share/dict/web2").read_text(encoding="utf-8", errors="ignore").split()
    except FileNotFoundError:
        web2 = []
    for w in web2:
        w = w.strip().lower()
        if not re.fullmatch(r"[a-z]{3,12}", w) or w in have:
            continue
        have.add(w)
        tail.append(w)
    tail.sort(key=lambda w: (len(w), w))
    tail = tail[: max(0, 5100 - len(common))]
    (SRC / "words_en.txt").write_text(
        EN_CORE + "\n" + "\n".join(common + tail) + "\n", encoding="utf-8")
    print(f"words_en source: {len(core_words)} curated + {len(common)} common "
          f"+ {len(tail)} web2 tail")

    (SRC / "medical.txt").write_text(
        "# Medical terms (single tokens; Zipf tail assigned at build)\n"
        + "\n".join(sorted(set(MEDICAL.split()))) + "\n", encoding="utf-8")

    rows = [{"w": p.split(":")[0], "tr": p.split(":")[1]}
            for p in NEPALI_NEW.split() if ":" in p]
    # Nepali is pipeline-owned since plan/03 (scripts/build_ne_pack.py ->
    # 8000-word packs/nepali.json v2.0.0). Never rewrite the pipeline's
    # packs/sources/nepali.csv back to this 223-word curated list.
    print(f"SKIP nepali.csv: pipeline-owned (plan/03); curated list kept in-code only")

    (SRC / "code_js.txt").write_text("\n".join(sorted(set(JS_NEW.split()))) + "\n",
                                     encoding="utf-8")
    (SRC / "code_rust.txt").write_text("\n".join(sorted(set(RUST_NEW.split()))) + "\n",
                                       encoding="utf-8")
    (SRC / "code_html.txt").write_text("\n".join(sorted(set(HTML_NEW.split()))) + "\n",
                                       encoding="utf-8")

    pairs = [p.split() for p in EMOJI_NEW.splitlines() if p.split()]
    flat = [item for pair in pairs for item in pair]
    it = iter(flat)
    erows = [{"w": e, "key": k} for e, k in zip(it, it)]
    with open(SRC / "emoji.csv", "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["w", "key"])
        w.writeheader()
        w.writerows(erows)

    toks = MATH_NEW.split()
    mrows = [{"w": toks[i], "key": toks[i + 1]} for i in range(0, len(toks), 2)]
    with open(SRC / "math.csv", "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["w", "key"])
        w.writeheader()
        w.writerows(mrows)
    print(f"sources: medical={len(set(MEDICAL.split()))} ne={len(rows)} "
          f"js={len(set(JS_NEW.split()))} rust={len(set(RUST_NEW.split()))} "
          f"html={len(set(HTML_NEW.split()))} emoji={len(erows)} math={len(mrows)}")


def run(*args):
    r = subprocess.run([*BP, *args], capture_output=True, text=True)
    print("$ build_pack.py", " ".join(args))
    print(r.stdout.strip())
    if r.returncode != 0:
        print(r.stderr.strip(), file=sys.stderr)
        sys.exit(r.returncode)


def drop_words(pack_name: str, words: set):
    """Remove placeholder-grade rows from a built pack (reproducible)."""
    from build_pack import encode_word  # same T9 map the builder uses
    p = ROOT / "packs" / f"{pack_name}.json"
    d = json.loads(p.read_text(encoding="utf-8"))
    before = len(d["words"])
    d["words"] = [e for e in d["words"] if e["w"] not in words]
    p.write_text(json.dumps(d, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"drop {pack_name}: -{before - len(d['words'])} "
          f"placeholders, total {len(d['words'])}")


def fix_emoji_keys():
    """Rewrite placeholder-grade emoji `key`s, re-materializing `seq`."""
    from build_pack import encode_word
    p = ROOT / "packs" / "emoji.json"
    d = json.loads(p.read_text(encoding="utf-8"))
    n = 0
    for e in d["words"]:
        new_key = EMOJI_KEY_FIX.get(e["w"])
        if new_key and e.get("key") != new_key:
            e["key"] = new_key
            e["seq"] = encode_word(new_key)
            n += 1
    p.write_text(json.dumps(d, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"fix emoji keys: {n} rows, total {len(d['words'])}")


def main():
    write_sources()
    P = ROOT / "packs"
    # Placeholder cleanup first (reproducible; expand-pack only adds).
    drop_words("code_rust", set(RUST_DROP.split()))
    drop_words("code_html", set(HTML_DROP.split()))
    drop_words("math", set(MATH_DROP.split()))
    fix_emoji_keys()
    run("build", "--id", "words", "--title", TITLES["words"], "--in",
        str(SRC / "words_en.txt"), "--out", str(P / "words_en.json"),
        "--lang", "en", "--cat", "words", "--version", "1.1.0",
        "--f0", "8000", "--alpha", "0.6", "--fmin", "120")
    run("build", "--id", "medical", "--title", TITLES["medical"], "--in",
        str(SRC / "medical.txt"), "--out", str(P / "medical.json"),
        "--lang", "en", "--cat", "medical", "--version", "1.0.0",
        "--f0", "6000", "--alpha", "0.8", "--fmin", "150")
    # Nepali is pipeline-owned (plan/03 v2.0.0, 8000 words): expanding the
    # curated 223 here would only rewrite freqs/sources, never add words
    # (dedupe keeps existing rows). Skip to keep provenance clean.
    print("$ build_pack.py expand-pack --pack nepali.json --in nepali.csv: SKIPPED (pipeline-owned)")
    run("expand-pack", "--pack", str(P / "code_js.json"), "--in", str(SRC / "code_js.txt"),
        "--f0", "4500", "--alpha", "0.7", "--fmin", "500")
    run("expand-pack", "--pack", str(P / "code_rust.json"), "--in", str(SRC / "code_rust.txt"),
        "--f0", "4500", "--alpha", "0.7", "--fmin", "500")
    run("expand-pack", "--pack", str(P / "code_html.json"), "--in", str(SRC / "code_html.txt"),
        "--f0", "4000", "--alpha", "0.7", "--fmin", "500")
    run("expand-pack", "--pack", str(P / "emoji.json"), "--in", str(SRC / "emoji.csv"),
        "--f0", "4500", "--alpha", "0.7", "--fmin", "800")
    run("expand-pack", "--pack", str(P / "math.json"), "--in", str(SRC / "math.csv"),
        "--f0", "4000", "--alpha", "0.7", "--fmin", "800")
    # bump expanded pack versions to 1.2.0 (nepali excluded: pipeline-owned v2.0.0)
    for name in ("code_js", "code_rust", "code_html", "emoji", "math"):
        p = P / f"{name}.json"
        d = json.loads(p.read_text(encoding="utf-8"))
        d["version"] = "1.2.0"
        p.write_text(json.dumps(d, ensure_ascii=False) + "\n", encoding="utf-8")
    run("validate", "--all")
    run("stats", "--all")


if __name__ == "__main__":
    main()
