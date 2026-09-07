# Operations

Runbooks cover DB corruption/recovery, migration failure, content rollback, update rollback, provider outage/terms disable, source refresh failure, crash loop/safe mode, repair-agent failure, gh auth failure and local model loss.

After a bounded startup crash loop, safe mode disables optional providers/MCP/new content candidate, opens diagnostics and protects uncertain DB state read-only while allowing backup/export. Automatic recovery never deletes learner data.
