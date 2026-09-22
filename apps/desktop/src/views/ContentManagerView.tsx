/**
 * Content manager: what the corpus holds, what it rests on, and withdrawal.
 *
 * Requirements: REQ-048 (review state, history, rollback, audit), REQ-056
 * (per-item provenance).
 *
 * ## What this view is for
 *
 * The corpus is generated and ingested by tools, and until now nothing inside the
 * application could show what those tools produced. A reviewer could not see an
 * item, could not see which source licensed it, and could not withdraw one. That
 * is the difference between a corpus and a corpus somebody is accountable for.
 *
 * ## Why the reason field is required
 *
 * Quarantine and reinstatement both write to the audit trail, and a trail whose
 * entries say "quarantined" with no reason answers the only question anyone asks
 * of it later. The buttons are disabled until a reason is typed rather than
 * writing a placeholder, because a placeholder is an entry that looks like a
 * record and is not one.
 */

import { useCallback, useEffect, useState } from "react";

import { useBackend } from "../ipc/Backend";
import type {
  ContentItemSummaryDto,
  ContentManagerDto,
  InstalledPackDto,
  ReviewEntryDto,
  SourceDto,
} from "../ipc/types";

/** How many items the view lists at once. */
const LIST_LIMIT = 50;

/**
 * Who is recorded as acting.
 *
 * This is a single-user local application with no accounts, so the honest value is
 * that the person at the machine did it. Inventing a name would be worse: the
 * audit trail is what a later reader trusts.
 */
export const LOCAL_ACTOR = "local-user";

type ManagerState =
  | { status: "loading" }
  | { status: "ready"; view: ContentManagerDto }
  | { status: "error"; message: string };

export function ContentManagerView() {
  const backend = useBackend();
  const [state, setState] = useState<ManagerState>({ status: "loading" });
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [trail, setTrail] = useState<{
    itemId: string;
    entries: ReviewEntryDto[];
  } | null>(null);
  const [packs, setPacks] = useState<InstalledPackDto[]>([]);
  const [packPath, setPackPath] = useState("");
  const [packNotice, setPackNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const view = await backend.client.contentManager(LIST_LIMIT);
      setState({ status: "ready", view });
    } catch (error) {
      setState({
        status: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [backend.client]);

  const loadPacks = useCallback(async () => {
    try {
      setPacks(await backend.client.contentPacks());
    } catch {
      // A registry that cannot be read shows as an empty list rather than as a
      // failure of the whole view: the corpus half of this surface is still usable,
      // and an install or rollback reports its own error.
      setPacks([]);
    }
  }, [backend.client]);

  useEffect(() => {
    void load();
    void loadPacks();
  }, [load, loadPacks]);

  async function installPack() {
    setBusy("pack-install");
    setActionError(null);
    setPackNotice(null);
    try {
      const report = await backend.client.contentPackInstall(packPath.trim());
      setPackPath("");
      await Promise.all([load(), loadPacks()]);
      setPackNotice(
        `Installed ${report.name} v${report.version}: ${report.installed} item(s) ` +
          `added, ${report.already_present} already present.`,
      );
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }

  async function rollbackPack(pack: InstalledPackDto) {
    setBusy(`pack-rollback-${pack.id}`);
    setActionError(null);
    setPackNotice(null);
    try {
      const rolled = await backend.client.contentPackRollback(pack.name);
      await Promise.all([load(), loadPacks()]);
      setPackNotice(
        `Rolled ${pack.name} back to v${rolled.version}: ${rolled.item_count} ` +
          `item(s) in service again.`,
      );
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }

  async function act(
    item: ContentItemSummaryDto,
    action: "quarantine" | "reinstate",
  ) {
    setBusy(item.id);
    setActionError(null);
    try {
      if (action === "quarantine") {
        await backend.client.contentQuarantine(item.id, LOCAL_ACTOR, reason);
      } else {
        await backend.client.contentReinstate(item.id, LOCAL_ACTOR, reason);
      }
      setReason("");
      await load();
      setTrail({
        itemId: item.id,
        entries: await backend.client.contentHistory(item.id),
      });
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }

  async function showTrail(item: ContentItemSummaryDto) {
    setActionError(null);
    try {
      setTrail({
        itemId: item.id,
        entries: await backend.client.contentHistory(item.id),
      });
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    }
  }

  if (state.status === "loading") {
    return (
      <section aria-label="Content manager">
        <p data-testid="content-loading">Loading the corpus…</p>
      </section>
    );
  }

  if (state.status === "error") {
    return (
      <section aria-label="Content manager">
        <p className="error" role="alert" data-testid="content-error">
          The corpus could not be read: {state.message}
        </p>
      </section>
    );
  }

  const { stats, sources, items } = state.view;
  const reasonMissing = reason.trim().length === 0;

  return (
    <section aria-label="Content manager">
      {/* No top-level heading here: `App.tsx` renders the view's `h2` from the
          registry, so a view that draws its own produces two headings with the
          same name and a screen reader announces the view twice. */}

      <section aria-labelledby="corpus-heading">
        <h3 id="corpus-heading">Corpus</h3>
        <p data-testid="corpus-summary">
          {stats.total} question(s) on this device, {stats.servable} in service,{" "}
          {stats.sources} source(s) recorded.
        </p>
        {stats.total === 0 ? (
          <p className="hint" data-testid="corpus-empty">
            No content yet. Practising a subtest prepares questions, and the
            ingestion tools add more from public-domain sources.
          </p>
        ) : (
          <ul data-testid="corpus-by-subtest">
            {stats.by_subtest.map((row) => (
              <li key={row.subtest}>
                {row.subtest}: {row.count}
              </li>
            ))}
          </ul>
        )}
        {stats.by_state.some((row) => row.state === "quarantined") && (
          <p className="hint" data-testid="corpus-quarantined-note">
            Quarantined items are held in the database but are not served.
          </p>
        )}
      </section>

      <section aria-labelledby="sources-heading">
        <h3 id="sources-heading">Sources</h3>
        {sources.length === 0 ? (
          <p className="hint" data-testid="sources-empty">
            Nothing recorded yet.
          </p>
        ) : (
          <table data-testid="sources-table">
            <thead>
              <tr>
                <th scope="col">Source</th>
                <th scope="col">Terms</th>
                <th scope="col">Items</th>
              </tr>
            </thead>
            <tbody>
              {sources.map((source: SourceDto) => (
                <tr key={source.id}>
                  <td>
                    <a href={source.url} rel="noreferrer noopener">
                      {source.title}
                    </a>
                  </td>
                  {/* The licence is shown because it is the question a reviewer
                      has: not which file, but on what terms it may be here. */}
                  <td>{source.licence}</td>
                  <td>{source.item_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section aria-labelledby="review-heading">
        <h3 id="review-heading">Withdraw or restore</h3>
        <p className="hint">
          Every change is recorded against your reason in the item&apos;s audit
          trail.
        </p>
        <label>
          Reason for the change
          <input
            type="text"
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            data-testid="reason-input"
          />
        </label>
        {reasonMissing && (
          <p className="hint" data-testid="reason-required">
            A reason is required before an item can be withdrawn or restored.
          </p>
        )}
        {actionError && (
          <p className="error" role="alert" data-testid="action-error">
            {actionError}
          </p>
        )}
      </section>

      <section aria-labelledby="packs-heading">
        <h3 id="packs-heading">Content packs</h3>
        <p className="hint">
          A pack is signed content with its sources and licences attached. Every
          check the installer makes has to pass before anything is written, and
          the version it replaces stays on disk so it can be put back.
        </p>

        {packs.length === 0 ? (
          <p className="hint" data-testid="packs-empty">
            No packs installed. The corpus on this device came from the
            ingestion tools or from practising a subtest.
          </p>
        ) : (
          <table data-testid="packs-table">
            <thead>
              <tr>
                <th scope="col">Pack</th>
                <th scope="col">Version</th>
                <th scope="col">Status</th>
                <th scope="col">Items</th>
                <th scope="col">Signature</th>
                <th scope="col">Action</th>
              </tr>
            </thead>
            <tbody>
              {packs.map((pack) => (
                <tr key={pack.id} data-testid={`pack-${pack.id}`}>
                  <td>{pack.name}</td>
                  <td>{pack.version}</td>
                  <td>{pack.status}</td>
                  <td>{pack.item_count}</td>
                  {/* The signature column is separate from the status column on
                      purpose: a pack can be active with an attestation that no
                      longer verifies, and that is the fact a reviewer needs. */}
                  <td data-testid={`pack-signature-${pack.id}`}>
                    {pack.signature_valid ? "verified" : "does not verify"}
                  </td>
                  <td>
                    <button
                      type="button"
                      disabled={
                        pack.status !== "active" ||
                        busy === `pack-rollback-${pack.id}`
                      }
                      onClick={() => void rollbackPack(pack)}
                      data-testid={`pack-rollback-${pack.id}`}
                    >
                      Roll back
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        <label>
          Install a pack from this machine
          <input
            type="text"
            value={packPath}
            onChange={(event) => setPackPath(event.target.value)}
            placeholder="path to a .vpack file"
            data-testid="pack-path-input"
          />
        </label>
        <button
          type="button"
          disabled={packPath.trim().length === 0 || busy === "pack-install"}
          onClick={() => void installPack()}
          data-testid="pack-install"
        >
          {busy === "pack-install" ? "Installing…" : "Install pack"}
        </button>
        {packNotice && (
          <p className="hint" data-testid="pack-notice" role="status">
            {packNotice}
          </p>
        )}
      </section>

      <section aria-labelledby="items-heading">
        <h3 id="items-heading">Questions</h3>
        {items.length === 0 ? (
          <p className="hint" data-testid="items-empty">
            No questions to review.
          </p>
        ) : (
          <ul data-testid="items-list">
            {items.map((item) => (
              <li key={item.id} data-testid={`item-${item.id}`}>
                <p className="subtest-tag">
                  {item.subtest} · {item.state} · {item.objective_id}
                </p>
                <p>{item.preview}</p>
                <p className="hint">
                  Answer: <strong>{item.correct_answer}</strong> · reviewed by{" "}
                  {item.reviewer || "nobody"}
                </p>
                <p className="source-line">
                  Cited: <code>{item.sources.join(", ")}</code>
                </p>
                <button
                  type="button"
                  onClick={() => void showTrail(item)}
                  data-testid={`history-${item.id}`}
                >
                  Audit trail
                </button>
                {item.state === "quarantined" ? (
                  <button
                    type="button"
                    disabled={reasonMissing || busy === item.id}
                    onClick={() => void act(item, "reinstate")}
                    data-testid={`reinstate-${item.id}`}
                  >
                    Return to service
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={reasonMissing || busy === item.id}
                    onClick={() => void act(item, "quarantine")}
                    data-testid={`quarantine-${item.id}`}
                  >
                    Withdraw from service
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      {trail && (
        <section aria-labelledby="trail-heading">
          <h3 id="trail-heading">Audit trail</h3>
          <p className="hint">
            <code>{trail.itemId}</code>
          </p>
          <ol data-testid="trail-list">
            {trail.entries.map((entry, index) => (
              <li key={`${entry.to_state}-${index}`}>
                {entry.from_state} → {entry.to_state} by {entry.actor}
                {entry.rationale ? `: ${entry.rationale}` : ""}
              </li>
            ))}
          </ol>
        </section>
      )}
    </section>
  );
}
