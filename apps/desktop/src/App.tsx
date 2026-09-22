/**
 * Application shell: keyboard-accessible navigation plus the active view.
 *
 * SPEC-004 requires keyboard-only operation. The navigation is therefore a real
 * landmark (`<nav>`) with an ordered list of buttons, a skip link, and roving
 * arrow-key focus, rather than a set of divs with click handlers.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DEFAULT_VIEW, VIEWS, viewDefinition, type ViewId } from "./app/views";
import {
  DEFAULT_A11Y_SETTINGS,
  a11yStyleVars,
  type A11ySettings,
} from "./accessibility/settings";
import { BackendProvider, useBackend } from "./ipc/Backend";
import { ProfileProvider, useActiveProfile } from "./state/ProfileContext";
import { AccessibleSettings } from "./views/AccessibleSettings";
import { OnboardingView } from "./views/OnboardingView";
import { ContentManagerView } from "./views/ContentManagerView";
import { PracticeContentView } from "./views/PracticeContentView";
import { SearchView } from "./views/SearchView";
import { ExamSimulatorView } from "./views/ExamSimulatorView";
import { ReadinessPanel } from "./views/ReadinessView";
import { ReviewQueueView } from "./views/ReviewQueueView";
import { PrivacyView } from "./views/PrivacyView";
import { SourcesView } from "./views/SourcesView";
import { TodayView } from "./views/TodayView";
import { reviewCards } from "./data/sample";
import "./styles.css";

export default function App() {
  // The backend and the active learner are established above every view,
  // because each of them depends on both and a view that fetched its own
  // learner could disagree with the navigation about who is studying.
  return (
    <BackendProvider>
      <ProfileProvider>
        <Shell />
      </ProfileProvider>
    </BackendProvider>
  );
}

/**
 * The application shell.
 *
 * Exported so a test can mount it inside an injected backend, which is how the
 * "the window renders but no command answers" path is exercised. The default
 * export installs the real providers around it.
 */
export function Shell() {
  const [view, setView] = useState<ViewId>(DEFAULT_VIEW);
  const [a11y, setA11y] = useState<A11ySettings>(DEFAULT_A11Y_SETTINGS);
  const [liveMessage, setLiveMessage] = useState("");
  const navRef = useRef<HTMLElement | null>(null);
  const backend = useBackend();

  const orderedViews = useMemo(
    () => [...VIEWS].sort((a, b) => a.order - b.order),
    [],
  );

  const navigate = useCallback((next: ViewId) => {
    setView(next);
    setLiveMessage(`Showing ${viewDefinition(next).label}`);
  }, []);

  /**
   * Roving arrow-key navigation within the nav list.
   *
   * Arrow keys move focus between views and Home/End jump to the ends, which is
   * the expected behaviour for a toolbar-style navigation.
   */
  const onNavKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>, index: number) => {
      let targetIndex: number | null = null;
      if (event.key === "ArrowDown" || event.key === "ArrowRight") {
        targetIndex = (index + 1) % orderedViews.length;
      } else if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
        targetIndex = (index - 1 + orderedViews.length) % orderedViews.length;
      } else if (event.key === "Home") {
        targetIndex = 0;
      } else if (event.key === "End") {
        targetIndex = orderedViews.length - 1;
      }
      if (targetIndex === null) return;

      event.preventDefault();
      const buttons =
        navRef.current?.querySelectorAll<HTMLButtonElement>(
          "button[data-view]",
        );
      buttons?.[targetIndex]?.focus();
    },
    [orderedViews.length],
  );

  // Keep the document title in step with the view so assistive technology and
  // window listings both reflect where the learner is.
  useEffect(() => {
    document.title = `VECTOR — ${viewDefinition(view).label}`;
  }, [view]);

  const styleVars = a11yStyleVars(a11y) as React.CSSProperties;

  return (
    <div
      className={`vector-app${a11y.highContrast ? " high-contrast" : ""}`}
      style={styleVars}
      data-testid="app-root"
    >
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>

      <header className="app-header">
        <h1>Project VECTOR</h1>
        <p className="app-subtitle">ASVAB/AFQT preparation</p>
      </header>

      {/*
        The interface rendering and the interface reaching the local core are
        different facts. When the packaged application is running but no command
        can be answered, the window would otherwise look normal while every
        number in it was unreadable — so say so plainly.
      */}
      {backend.boundaryError && (
        <p
          role="alert"
          className="status-notice status-error"
          data-testid="ipc-broken"
        >
          The interface could not reach the local core, so no study data can be
          read or saved: {backend.boundaryError}
        </p>
      )}

      <div className="app-body">
        <nav aria-label="Primary" ref={navRef} data-testid="primary-nav">
          <ul className="nav-list">
            {orderedViews.map((definition, index) => (
              <li key={definition.id}>
                <button
                  type="button"
                  data-view={definition.id}
                  aria-current={view === definition.id ? "page" : undefined}
                  onClick={() => navigate(definition.id)}
                  onKeyDown={(e) => onNavKeyDown(e, index)}
                >
                  {definition.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>

        <main id="main-content" tabIndex={-1} data-testid="main-content">
          <h2 data-testid="view-heading">{viewDefinition(view).label}</h2>
          <ViewBody view={view} a11y={a11y} onA11yChange={setA11y} />
        </main>
      </div>

      {/*
        A polite live region announces view changes. Without it a screen-reader
        user gets no feedback that navigation happened.
      */}
      <div
        role="status"
        aria-live="polite"
        className="visually-hidden"
        data-testid="live-region"
      >
        {liveMessage}
      </div>
    </div>
  );
}

interface ViewBodyProps {
  view: ViewId;
  a11y: A11ySettings;
  onA11yChange: (settings: A11ySettings) => void;
}

function ViewBody({ view, a11y, onA11yChange }: ViewBodyProps) {
  // Attempts are recorded against the learner the application has selected.
  const { profile } = useActiveProfile();

  switch (view) {
    case "onboarding":
      return <OnboardingView />;
    case "today":
      return <TodayView />;
    case "accessibility":
      return <AccessibleSettings settings={a11y} onChange={onA11yChange} />;
    case "content":
      return <ContentManagerView />;
    case "practice":
      // Real generated items, loaded from the command boundary. This used to
      // pass `sampleQuestions`, which is how the app appeared to have content.
      return <PracticeContentView learnerId={profile?.id} />;
    case "search":
      return <SearchView />;
    case "cat":
      return <ExamSimulatorView form="cat" />;
    case "paper":
      return <ExamSimulatorView form="paper" />;
    case "readiness":
      return <ReadinessPanel />;
    case "review":
      return <ReviewQueueView cards={reviewCards} />;
    case "evidence":
      return <SourcesView />;
    case "privacy":
      return <PrivacyView />;
    default:
      return <PlaceholderView view={view} />;
  }
}

const PENDING_DESCRIPTIONS: Partial<Record<ViewId, string>> = {
  diagnostic: "Take a cross-domain diagnostic to establish mastery estimates.",
  lesson: "Read lessons with worked solutions and cited sources.",
  explore: "Browse jobs and their current, sourced composite targets.",
  tutor: "Ask the local model, grounded in your evidence vault.",
  content: "Manage signed content packs and updates.",
  providers: "Configure model transports and review provider terms.",
};

function PlaceholderView({ view }: { view: ViewId }) {
  const label = viewDefinition(view).label;
  const description =
    PENDING_DESCRIPTIONS[view] ?? "This view is part of the product surface.";
  return (
    <section aria-label={label}>
      <p>{description}</p>
      {/*
        A deliberately visible marker rather than a hidden stub: these views are
        registered in navigation, but the behaviour behind them is delivered by
        a later node. Saying so is more honest than faking content.
      */}
      <p className="pending-note" data-testid="view-pending">
        Detailed {label.toLowerCase()} content is delivered by a later node.
      </p>
    </section>
  );
}
