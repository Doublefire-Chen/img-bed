import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import {
  type ApiToken,
  api,
  type CreatedApiToken,
  TOKENS_KEY,
  UPLOAD_ENDPOINT,
  UPLOAD_FIELD,
} from "../api";
import { formatDate, formatDateTime } from "../format";
import { copyText } from "../links";

/**
 * API Tokens: create a named token (its secret is shown once), list tokens
 * with their created / last used / revoked dates, and revoke them.
 */
export function TokensPage() {
  const query = useQuery({ queryKey: TOKENS_KEY, queryFn: api.tokens });
  const [created, setCreated] = useState<CreatedApiToken | null>(null);
  const tokens = query.data?.tokens ?? [];
  const active = tokens.filter((t) => t.revoked_at === null);
  const revoked = tokens.filter((t) => t.revoked_at !== null);

  return (
    <section className="tokens-section">
      <h1>API Tokens</h1>
      <p className="muted small">
        An API Token lets a tool such as PicGo, ShareX, Typora or a script upload for you. It can
        upload and nothing else: it can't list, change or delete Images, or manage tokens. Revoke a
        token as soon as you no longer need it or it may have leaked.
      </p>

      <CreateTokenForm onCreated={setCreated} />
      {created && <NewSecret token={created} onDone={() => setCreated(null)} />}

      {query.isError && (
        <p className="error" role="alert">
          Could not load the API Tokens: {query.error.message}
        </p>
      )}
      {query.isPending && <p className="muted">Loading…</p>}

      {query.data && (
        <>
          <h2>Active</h2>
          {active.length === 0 ? (
            <p className="muted small">No active API Tokens.</p>
          ) : (
            <ul className="token-list">
              {active.map((token) => (
                <TokenItem key={token.id} token={token} />
              ))}
            </ul>
          )}
          {revoked.length > 0 && (
            <>
              <h2>Revoked</h2>
              <ul className="token-list">
                {revoked.map((token) => (
                  <TokenItem key={token.id} token={token} />
                ))}
              </ul>
            </>
          )}
        </>
      )}

      <UsageHelp />
    </section>
  );
}

function CreateTokenForm({ onCreated }: { onCreated: (token: CreatedApiToken) => void }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const create = useMutation({
    mutationFn: () => api.createToken(name.trim()),
    onSuccess: (token) => {
      setName("");
      onCreated(token);
      void queryClient.invalidateQueries({ queryKey: TOKENS_KEY });
    },
  });

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) return;
    create.mutate();
  };

  return (
    <form onSubmit={onSubmit}>
      <div className="token-form">
        <input
          type="text"
          aria-label="Token name"
          placeholder="Name, e.g. PicGo on my laptop"
          maxLength={100}
          autoComplete="off"
          aria-invalid={create.isError ? "true" : "false"}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            create.reset();
          }}
        />
        <button type="submit" className="button-primary" disabled={!name.trim() || create.isPending}>
          {create.isPending ? "Creating…" : "Create token"}
        </button>
      </div>
      {create.isError && (
        <p className="error small" role="alert">
          {create.error.message}
        </p>
      )}
    </form>
  );
}

/** The new token's secret, shown only until the Owner dismisses it. */
function NewSecret({ token, onDone }: { token: CreatedApiToken; onDone: () => void }) {
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async () => {
    try {
      await copyText(token.secret);
      setFailed(false);
      setCopied(true);
    } catch {
      setFailed(true);
    }
  };

  return (
    <div className="token-secret" role="status">
      <p>
        <strong>{token.name}</strong> created. Copy its secret now:{" "}
        <strong>you won't see it again.</strong> If you lose it, revoke the token and create a new
        one.
      </p>
      <div className="token-secret-value">
        <code aria-label="Token secret">{token.secret}</code>
        <button type="button" className="button-small button-primary" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {failed && <p className="error small">Copy failed; select the secret instead.</p>}
      <div>
        <button type="button" className="button-small button-quiet" onClick={onDone}>
          I've saved it
        </button>
      </div>
    </div>
  );
}

function When({ at, never }: { at: number | null; never: string }) {
  if (at === null) return <>{never}</>;
  return <time title={formatDateTime(at)}>{formatDate(at)}</time>;
}

function TokenItem({ token }: { token: ApiToken }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const revoke = useMutation({
    mutationFn: () => api.revokeToken(token.id),
    onSuccess: () => {
      setConfirming(false);
      void queryClient.invalidateQueries({ queryKey: TOKENS_KEY });
    },
  });
  const isRevoked = token.revoked_at !== null;

  return (
    <li className={`token-item${isRevoked ? " token-revoked" : ""}`}>
      <div className="token-row">
        <div>
          <p>
            <strong>{token.name}</strong>
            {isRevoked && <span className="badge small">Revoked</span>}
          </p>
          <p className="muted small">
            Created <When at={token.created_at} never="" /> · Last used{" "}
            <When at={token.last_used_at} never="never" />
            {isRevoked && (
              <>
                {" "}
                · Revoked <When at={token.revoked_at} never="" />
              </>
            )}
          </p>
        </div>
        {!isRevoked && !confirming && (
          <button
            type="button"
            className="button-small button-danger"
            onClick={() => {
              revoke.reset();
              setConfirming(true);
            }}
          >
            Revoke
          </button>
        )}
      </div>
      {confirming && (
        <div className="confirm" role="alertdialog" aria-label="Revoke API Token">
          <p className="small">
            Revoke <strong>{token.name}</strong>? Tools using it can no longer upload. This cannot
            be undone; Images already uploaded with it are not affected.
          </p>
          {revoke.isError && (
            <p className="error small" role="alert">
              {revoke.error.message}
            </p>
          )}
          <div className="copy-buttons">
            <button
              type="button"
              className="button-small button-danger-solid"
              disabled={revoke.isPending}
              autoFocus
              onClick={() => revoke.mutate()}
            >
              {revoke.isPending ? "Revoking…" : "Revoke"}
            </button>
            <button
              type="button"
              className="button-small button-quiet"
              disabled={revoke.isPending}
              onClick={() => setConfirming(false)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </li>
  );
}

/** How to upload with a token: the endpoint, header, field and a curl example. */
function UsageHelp() {
  const endpoint = `${window.location.origin}${UPLOAD_ENDPOINT}`;
  const curl = `curl -H "Authorization: Bearer imgbed_…" \\\n  -F "${UPLOAD_FIELD}=@photo.png" \\\n  ${endpoint}`;
  return (
    <aside className="token-help">
      <h2>Using a token</h2>
      <pre>
        <code>{curl}</code>
      </pre>
      <dl>
        <dt>Endpoint</dt>
        <dd>
          <code>POST {endpoint}</code>
        </dd>
        <dt>Header</dt>
        <dd>
          <code>Authorization: Bearer &lt;secret&gt;</code>
        </dd>
        <dt>Body</dt>
        <dd>
          <code>multipart/form-data</code>, file field <code>{UPLOAD_FIELD}</code> (repeat it for
          several files)
        </dd>
        <dt>Link</dt>
        <dd>
          <code>results[0].image.url</code> in the JSON response (
          <code>results[0].duplicate</code> is true for a Duplicate Upload)
        </dd>
      </dl>
    </aside>
  );
}
