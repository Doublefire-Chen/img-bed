import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import {
  ApiError,
  api,
  MAX_USERNAME_LEN,
  MIN_PASSWORD_LEN,
  type Settings,
  SETTINGS_KEY,
} from "../api";
import { SESSION_KEY, useSession } from "../session";

const CUSTOM = "custom";

/** The Owner's settings: the Path Template, the username and the password. */
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
      <ChangeUsernameForm />
      <ChangePasswordForm />
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

function describePasswordError(error: unknown): string {
  if (error instanceof ApiError) {
    switch (error.code) {
      case "wrong_current_password":
        return "The current password is wrong.";
      case "password_too_short":
        return `The new password must be at least ${MIN_PASSWORD_LEN} characters.`;
      case "too_many_attempts":
        return `Too many wrong passwords. ${error.message.replace(/^too many failed [^;]*; /, "")}`;
    }
    return error.message;
  }
  return "Could not reach the server.";
}

/** Mirrors the server's username rules; returns a message, or null if fine. */
function usernameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return "The username must not be empty.";
  if ([...trimmed].length > MAX_USERNAME_LEN)
    return `The username must be at most ${MAX_USERNAME_LEN} characters.`;
  if (/[\s\p{Cc}]/u.test(trimmed)) return "The username must not contain spaces.";
  return null;
}

function describeUsernameError(error: unknown): string {
  if (error instanceof ApiError && error.code === "invalid_username") {
    return `${error.message.charAt(0).toUpperCase()}${error.message.slice(1)}.`;
  }
  return describePasswordError(error);
}

/** Changes the Owner's username (login matches it in any case); no session ends. */
function ChangeUsernameForm() {
  const session = useSession();
  const queryClient = useQueryClient();
  const currentName = session.data?.username ?? "";
  const [name, setName] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const value = name ?? currentName;

  const change = useMutation({
    mutationFn: () => api.changeUsername(password, value),
    onSuccess: (data) => {
      setPassword("");
      setName(null);
      setDone(true);
      queryClient.setQueryData(SESSION_KEY, (old: typeof session.data) =>
        old ? { ...old, username: data.username } : old,
      );
    },
    onError: (error) => {
      if (error instanceof ApiError && error.code === "wrong_current_password") setPassword("");
    },
  });

  const edited = () => {
    setLocalError(null);
    setDone(false);
    change.reset();
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    const problem = usernameProblem(value);
    if (problem) {
      setLocalError(problem);
      return;
    }
    change.mutate();
  };

  const error = localError ?? (change.isError ? describeUsernameError(change.error) : null);

  return (
    <form className="password-form" method="post" onSubmit={onSubmit}>
      <h2>Change username</h2>
      <p className="muted small">
        The username you log in with. Upper and lower case don't matter when logging in. Nobody is
        signed out.
      </p>
      <label className="field">
        <span>Username</span>
        <input
          type="text"
          name="username"
          autoComplete="username"
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          required
          maxLength={MAX_USERNAME_LEN}
          value={value}
          onChange={(e) => {
            setName(e.target.value);
            edited();
          }}
        />
      </label>
      <label className="field">
        <span>Current password</span>
        <input
          type="password"
          name="password"
          autoComplete="current-password"
          required
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
            edited();
          }}
        />
      </label>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="settings-actions">
        <button
          type="submit"
          className="button-primary"
          disabled={change.isPending || value.trim() === currentName}
        >
          {change.isPending ? "Changing…" : "Change username"}
        </button>
        {done && (
          <span className="muted small" role="status">
            Username changed to “{currentName}”.
          </span>
        )}
      </div>
    </form>
  );
}

/** Changes the Owner's password; other sessions are signed out, this one stays. */
function ChangePasswordForm() {
  const username = useSession().data?.username ?? "";
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const [done, setDone] = useState<number | null>(null);

  const change = useMutation({
    mutationFn: () => api.changePassword(current, next),
    onSuccess: (data) => {
      setCurrent("");
      setNext("");
      setConfirm("");
      setDone(data.ended_sessions);
    },
    onError: (error) => {
      if (error instanceof ApiError && error.code === "wrong_current_password") setCurrent("");
    },
  });

  const edited = () => {
    setLocalError(null);
    setDone(null);
    change.reset();
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if ([...next].length < MIN_PASSWORD_LEN) {
      setLocalError(`The new password must be at least ${MIN_PASSWORD_LEN} characters.`);
      return;
    }
    if (next !== confirm) {
      setLocalError("The new passwords do not match.");
      return;
    }
    change.mutate();
  };

  const mismatch = confirm !== "" && next !== confirm;
  const error = localError ?? (change.isError ? describePasswordError(change.error) : null);

  return (
    <form className="password-form" method="post" onSubmit={onSubmit}>
      <h2>Change password</h2>
      {/* Tells password managers which account the new password belongs to. */}
      <input
        type="text"
        name="username"
        autoComplete="username"
        value={username}
        readOnly
        hidden
      />
      <p className="muted small">
        Changing the password signs out every other browser. This one stays logged in. API Tokens
        keep working; revoke them on the API Tokens page if needed.
      </p>
      <label className="field">
        <span>Current password</span>
        <input
          type="password"
          autoComplete="current-password"
          required
          value={current}
          onChange={(e) => {
            setCurrent(e.target.value);
            edited();
          }}
        />
      </label>
      <label className="field">
        <span>New password</span>
        <input
          type="password"
          autoComplete="new-password"
          required
          minLength={MIN_PASSWORD_LEN}
          value={next}
          onChange={(e) => {
            setNext(e.target.value);
            edited();
          }}
        />
        <span className="muted small">At least {MIN_PASSWORD_LEN} characters.</span>
      </label>
      <label className="field">
        <span>Confirm new password</span>
        <input
          type="password"
          autoComplete="new-password"
          required
          aria-invalid={mismatch ? "true" : "false"}
          value={confirm}
          onChange={(e) => {
            setConfirm(e.target.value);
            edited();
          }}
        />
        {mismatch && <span className="error small">The new passwords do not match.</span>}
      </label>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="settings-actions">
        <button type="submit" className="button-primary" disabled={change.isPending || mismatch}>
          {change.isPending ? "Changing…" : "Change password"}
        </button>
        {done !== null && (
          <span className="muted small" role="status">
            Password changed.{" "}
            {done === 0
              ? "No other sessions were signed in."
              : `${done} other ${done === 1 ? "session was" : "sessions were"} signed out.`}
          </span>
        )}
      </div>
    </form>
  );
}
