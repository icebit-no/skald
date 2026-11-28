Skald for Windows

Norsk diktering på maskinen din: hold høyre Ctrl, snakk, og slipp. Teksten settes inn der
markøren er. Talemodellene er laget av Nasjonalbiblioteket og kjører her, på maskinen.
Ingenting sendes fra maskinen.


Kom i gang

1. Start skald.exe. Første gang åpner «Kom i gang med Skald», som laster ned talemodellen
   (3,1 GB) og lar deg prøve.
2. Hold høyre Ctrl, snakk, og slipp. Ctrl+Win+X starter og stopper et håndfritt opptak.
3. ᚨ i systemstatusfeltet har menyen: mikrofon, siste tekster, skriftspråk, ordskatt og
   innstillinger.


Skjermkortet

Skald regner på skjermkortet med Vulkan, som driverne fra NVIDIA, AMD og Intel har med seg.
Uten en skjermkortdriver med Vulkan regner talemodellen på prosessoren og bruker lengre tid.
Under «Om» i innstillingene står det hva talemodellen regner på.


Det Skald trenger

- Windows 10 eller 11, 64-bit.
- Microsoft Visual C++ Redistributable for Visual Studio 2015–2022 (x64). De fleste maskiner
  har den allerede; ellers lastes den ned fra Microsoft:
  https://aka.ms/vs/17/release/vc_redist.x64.exe


Vinduer som kjører som administrator

Windows lar ikke Skald sette inn tekst i et vindu som kjører som administrator, og Skald
hører ikke tasten mens et slikt vindu er foran. Teksten ligger da i «Siste tekster».


Kommandolinjen

skald.exe er også kommandolinjen, for deg og for KI-agenter. skald --help viser kommandoene.


Avinstallere

Slett mappen. Innstillingene ligger i %APPDATA%\Skald, og talemodellen og historikken i
%LOCALAPPDATA%\Skald.


Lisens

Skald er fri programvare med åpen kildekode under MIT-lisensen (LICENSE.txt). Talemodellene
er laget av Nasjonalbiblioteket og har Apache 2.0-lisens. Det andre Skald bygger på, står i
THIRD-PARTY-NOTICES.txt.

https://skald.icebit.app
https://github.com/icebit-no/skald
