import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { ApiError, api, type Settings, SETTINGS_KEY } from "../api";

const CUSTOM = "custom";

/** The Owner's settings; for now the Path Template. */
export function SettingsPage() {
  const query = useQuery({ queryKey: SETTINGS_KEY, queryFn: api.settings });

  return (
    <section className="settings-section">
      <h1>Settings</h1>
      {query.isError ? (
        <p className="error" role="alert">
          Could not load the settings: {query.error.message}
        </p>
      ) : query.data ? (
        <PathTemplateForm settings={query.data} />
      ) : (
        <p className="muted">Loading…</p>
      )}
    </section>
  );
}

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), ms);
    return () => window.clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

function PathTemplateForm({ settings }: { settings: Settings }) {
  const queryClient = useQueryClient();
  const [choice, setChoice] = useState<string>(settings.preset ?? CUSTOM);
  const [custom, setCustom] = useState(settings.preset ? "" : settings.path_template);
  const [saved, setSaved] = useState(false);

  const preset = settings.presets.find((p) => p.id === choice);
  const template = preset ? preset.template : custom.trim();
  const dirty = template !== settings.path_template;

  // Custom templates are checked and previewed by the server as the Owner
  // types, so the page shows exactly what saving would do.
  const debounced = useDebounced(template, 300);
  const preview = useQuery({
    queryKey: [...SETTINGS_KEY, "preview", debounced],
    queryFn: () => api.previewPathTemplate(debounced),
    enabled: choice === CUSTOM && debounced !== "",
    retry: false,
    placeholderData: keepPreviousData,
  });

  const save = useMutation({
    mutationFn: () => api.savePathTemplate(template),
    onSuccess: (data) => {
      queryClient.setQueryData(SETTINGS_KEY, data);
      setSaved(true);
    },
  });

  const choose = (id: string) => {
    setChoice(id);
    setSaved(false);
    save.reset();
    // Start a Custom template from the current one rather than from nothing.
    if (id === CUSTOM && !custom) setCustom(settings.path_template);
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (!template || !dirty) return;
    save.mutate();
  };

  const settled = choice !== CUSTOM || (debounced === template && !preview.isFetching);
  const previewError = choice === CUSTOM && preview.error instanceof ApiError ? preview.error : null;
  const error = save.error ?? (settled ? previewError : null);

  let example: string | null = null;
  if (preset) example = preset.example;
  else if (template && preview.data && !preview.isError) example = preview.data.example;

  return (
    <form onSubmit={onSubmit}>
      <h2>Path Template</h2>
      <p className="muted small">
        New uploads get their first Path from this template. Changing it affects only future
        uploads (from this page and from API Tokens); existing Paths never change.
      </p>

      <fieldset className="template-choices">
        <legend className="small muted">Choose a Preset or write your own</legend>
        {settings.presets.map((p) => (
          <label
            key={p.id}
            className={`template-choice${choice === p.id ? " template-choice-selected" : ""}`}
          >
            <input
              type="radio"
              name="path-template"
              value={p.id}
              checked={choice === p.id}
              onChange={() => choose(p.id)}
            />
            <span className="template-choice-body">
              <span>
                <strong>{p.name}</strong>
                {p.id === settings.preset && <span className="muted small"> (current)</span>}
              </span>
              <code className="small">{p.template}</code>
              <span className="muted small">e.g. /{p.example}</span>
            </span>
          </label>
        ))}
        <label
          className={`template-choice${choice === CUSTOM ? " template-choice-selected" : ""}`}
        >
          <input
            type="radio"
            name="path-template"
            value={CUSTOM}
            checked={choice === CUSTOM}
            onChange={() => choose(CUSTOM)}
          />
          <span className="template-choice-body">
            <span>
              <strong>Custom</strong>
              {settings.preset === null && <span className="muted small"> (current)</span>}
            </span>
            {choice === CUSTOM && (
              <input
                className="template-input"
                type="text"
                aria-label="Custom Path Template"
                aria-invalid={error ? "true" : "false"}
                spellCheck={false}
                autoComplete="off"
                autoFocus={settings.preset !== null}
                value={custom}
                placeholder="{random:6}.{ext}"
                onChange={(e) => {
                  setCustom(e.target.value);
                  setSaved(false);
                  save.reset();
                }}
              />
            )}
          </span>
        </label>
      </fieldset>

      <div className="template-preview" aria-live="polite">
        {error ? (
          <p className="error" role="alert">
            {error.message}
          </p>
        ) : example ? (
          <p>
            <span className="muted small">Example Path for an upload of “holiday photo.jpg”: </span>
            <code>/{example}</code>
          </p>
        ) : (
          <p className="muted small">
            {template ? "Checking…" : "Write a template to see an example Path."}
          </p>
        )}
      </div>

      <div className="settings-actions">
        <button
          type="submit"
          className="button-primary"
          disabled={!dirty || !template || save.isPending || (choice === CUSTOM && !!previewError)}
        >
          {save.isPending ? "Saving…" : "Save"}
        </button>
        {saved && !dirty && <span className="muted small">Saved. The next upload uses it.</span>}
        {!dirty && !saved && <span className="muted small">This is the current template.</span>}
      </div>

      <table className="variables">
        <caption className="small muted">
          Variables (a template needs {"{random:N}"} or {"{hash:N}"})
        </caption>
        <tbody>
          <Variable name="{random:N}" meaning="N random letters and digits (1–64)" />
          <Variable name="{hash:N}" meaning="the first N characters of the file's SHA-256 (1–64)" />
          <Variable name="{Y} {m} {d}" meaning="upload year, month, day (UTC)" />
          <Variable name="{name}" meaning="the original filename, cleaned" />
          <Variable name="{ext}" meaning="the extension of the detected type (jpg, png, …)" />
        </tbody>
      </table>
      <p className="muted small">
        Literal text may use letters, digits, - _ . ~ and / (for folders).
      </p>
    </form>
  );
}

function Variable({ name, meaning }: { name: string; meaning: string }) {
  return (
    <tr>
      <td>
        <code>{name}</code>
      </td>
      <td className="muted">{meaning}</td>
    </tr>
  );
}
