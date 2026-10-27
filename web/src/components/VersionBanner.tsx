import { useQuery } from "@tanstack/react-query";
import { api, APP_VERSION } from "../api";

/**
 * Warns when this frontend was built for a different server version, e.g.
 * after a half-finished upgrade.
 */
export function VersionBanner() {
  const { data } = useQuery({ queryKey: ["info"], queryFn: api.info, staleTime: Infinity });
  if (!data || data.version === APP_VERSION) return null;
  return (
    <div className="banner banner-warning" role="alert">
      Version mismatch: this admin frontend is <strong>v{APP_VERSION}</strong> but the server is{" "}
      <strong>v{data.version}</strong>. Finish the upgrade so both match, then reload.
    </div>
  );
}
