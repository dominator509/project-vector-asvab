# Assumptions

| assumption | reason | risk | verification | blocks |
|---|---|---|---|---|
| exact dependency versions selected in EP-000 | blueprint must not invent registry versions | compatibility/license drift | verify current registries/licensing then lock | install |
| Windows 11 x64 release baseline | supported current Windows | legacy user exclusion | clean VM + hardware matrix | no |
| human UAT not yet provisioned | no evidence supplied | GA cannot be GO | named representative UAT evidence | GA |
| manual accessibility validators not yet provisioned | no evidence supplied | GA accessibility gate open | NVDA/Narrator + keyboard review | GA |
| exact bundled GGUF not selected | user requested local option, not model artifact | redistribution risk | exact model-card/license audit | bundled model only |
| Claude native use remains conditional | provider terms require ongoing verification | terms/account risk | dated terms review | Claude adapter |
| Gemini Notebook Enterprise license absent | no license supplied | enterprise connector unavailable | IAM/license preflight | connector only |
| signing certificate absent | no credential supplied | public release blocked | protected signing preflight | GA |
| no calibration cohort exists yet | greenfield | exact readiness prediction unsupported | validation study | precision claims |
