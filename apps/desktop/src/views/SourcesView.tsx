/**
 * Sources / evidence viewer (REQ-020, SPEC-004).
 *
 * The vault is hash-addressed and append-only: a snapshot is identified by its
 * content hash, re-storing identical content returns the existing record, and
 * rows are immutable at the database level. This view therefore presents the
 * records as read-only evidence and never offers an edit control, because an
 * edit that silently failed would be worse than no control at all.
 *
 * Retrieval status and trust are shown per row: a source that was never
 * successfully retrieved is not evidence, and the learner has to be able to see
 * that rather than infer it from a title.
 */

import { AsyncBoundary, useAsync, useBackend } from "../ipc/Backend";
import type { EvidenceDto } from "../ipc/types";

export function SourcesView() {
  const backend = useBackend();
  const records = useAsync(
    (client) => client.evidenceList(),
    [backend.available],
    backend.available,
  );

  return (
    <section aria-labelledby="sources-heading">
      <h3 id="sources-heading">Source snapshots</h3>

      <p data-testid="vault-explainer">
        Every claim in VECTOR traces to a stored snapshot: the address it came
        from, the hash of the content, its licence and when it took effect.
        Snapshots are append-only and cannot be edited after they are stored.
      </p>

      <AsyncBoundary state={records.state} reload={records.reload}>
        {(rows) =>
          rows.length === 0 ? (
            <p data-testid="vault-empty">
              No sources are stored yet. Sources arrive with signed content
              packs.
            </p>
          ) : (
            <table data-testid="vault-table">
              <caption>
                {rows.length} stored source snapshot
                {rows.length === 1 ? "" : "s"}, newest first.
              </caption>
              <thead>
                <tr>
                  <th scope="col">Title</th>
                  <th scope="col">Licence</th>
                  <th scope="col">Effective</th>
                  <th scope="col">Retrieval</th>
                  <th scope="col">Trust</th>
                  <th scope="col">Content hash</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <EvidenceRow key={row.id} record={row} />
                ))}
              </tbody>
            </table>
          )
        }
      </AsyncBoundary>
    </section>
  );
}

export function EvidenceRow({ record }: { record: EvidenceDto }) {
  // A source that was not successfully retrieved has no verified content, so
  // its hash is not evidence of anything. Flag it rather than printing it
  // beside the others as though it were equivalent.
  const retrieved = record.retrieval_status.toLowerCase() === "retrieved";

  return (
    <tr data-testid={`evidence-${record.id}`}>
      <th scope="row">
        <a href={record.url} rel="noreferrer noopener" target="_blank">
          {record.title}
        </a>
        <span className="field-help">{record.url}</span>
      </th>
      <td>{record.license || "unspecified"}</td>
      <td>{record.effective_date || "unknown"}</td>
      <td>
        {record.retrieval_status}
        {!retrieved && (
          <span className="flag" data-testid={`unretrieved-${record.id}`}>
            {" "}
            — not verified
          </span>
        )}
      </td>
      <td>{record.trust.toFixed(2)}</td>
      <td>
        <code>{shortHash(record.content_hash)}</code>
      </td>
    </tr>
  );
}

/** Show enough of a hash to identify it, without pretending to be the whole. */
function shortHash(hash: string): string {
  const body = hash.includes(":") ? hash.slice(hash.indexOf(":") + 1) : hash;
  return body.length <= 16 ? hash : `${body.slice(0, 12)}…`;
}
