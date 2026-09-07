# Deployment

Build Windows candidate from clean pinned checkout. Artifact identity binds git SHA, lockfiles/toolchains, migrations, content manifests, third-party notices and SBOM.

Flow: build candidate -> exact-artifact tests -> protected signing -> signature/hash verification -> clean install -> upgrade from supported prior version -> rollback drill -> publish only after GraphLock GO and human release authorization. Auto-deploy is no.
