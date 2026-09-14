/**
 * Search view (REQ-044): local full-text search across lessons, error notebook
 * entries and evidence records. Queries never leave the machine.
 */

import { useMemo, useState } from "react";
import { searchCorpus } from "../data/sample";
import { search, type SearchCategory } from "../search/search";

const CATEGORY_LABELS: Record<SearchCategory, string> = {
  lesson: "Lesson",
  error: "Error notebook",
  evidence: "Source",
};

export function SearchView() {
  const [query, setQuery] = useState("");
  const [categories, setCategories] = useState<SearchCategory[]>([]);

  const results = useMemo(
    () =>
      search(searchCorpus, query, {
        categories: categories.length > 0 ? categories : undefined,
      }),
    [query, categories],
  );

  const toggleCategory = (category: SearchCategory) => {
    setCategories((prev) =>
      prev.includes(category)
        ? prev.filter((c) => c !== category)
        : [...prev, category],
    );
  };

  return (
    <section aria-labelledby="search-heading">
      <h3 id="search-heading">Search</h3>

      <label htmlFor="search-input">Search lessons, errors and sources</label>
      <input
        id="search-input"
        type="search"
        value={query}
        placeholder="e.g. rate problems"
        onChange={(e) => setQuery(e.target.value)}
      />

      <fieldset>
        <legend>Filter by type</legend>
        {(Object.keys(CATEGORY_LABELS) as SearchCategory[]).map((category) => (
          <label key={category} className="option-row">
            <input
              type="checkbox"
              checked={categories.includes(category)}
              onChange={() => toggleCategory(category)}
            />
            <span>{CATEGORY_LABELS[category]}</span>
          </label>
        ))}
      </fieldset>

      {/*
        aria-live so a screen-reader user hears the result count change as they
        type, rather than having to navigate to discover it.
      */}
      <p aria-live="polite" data-testid="result-count">
        {query.trim() === ""
          ? "Type to search."
          : `${results.length} result${results.length === 1 ? "" : "s"}.`}
      </p>

      {results.length > 0 && (
        <ul className="search-results" data-testid="search-results">
          {results.map((result) => (
            <li key={result.document.id}>
              <h4>{result.document.title}</h4>
              <p className="result-category">
                {CATEGORY_LABELS[result.document.category]}
                {result.document.sourceId
                  ? ` — ${result.document.sourceId}`
                  : ""}
              </p>
              <p className="snippet">{result.snippet}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
