### the honest update verifies
exit: 0
{
  "format": "vector-update-1",
  "app": "vector-desktop",
  "version": "0.2.0",
  "min_running_version": "0.1.0",
  "published_at": "2026-09-23T14:37:37.626744100+00:00",
  "artifact": {
    "file_name": "vector-desktop-0.2.0.exe",
    "sha256": "sha256:cb44575b12446b1253a3ce16456c6708a573efeda501d6f47695616b4881fa13",
    "bytes": 27
  },
  "notes_url": null
}

### an untrusted signer is refused
exit: 1


### an offer that is not newer is refused
exit: 1


### stage and apply
exit: 0
{"displaced":"C:\\tmp\\vector-update-e2e\\install\\vector-desktop.pre-update","staged":"C:\\tmp\\vector-update-e2e\\install\\.update-staged-0.2.0","target":"C:\\tmp\\vector-update-e2e\\install\\vector-desktop.exe"}

target after apply: the published 0.2.0 bytes
backup after apply: 0.1.0 installed bytes

### roll back
exit: 0
{"restored_from":"C:\\tmp\\vector-update-e2e\\install\\vector-desktop.pre-update"}

target after rollback: 0.1.0 installed bytes
backup consumed: True

