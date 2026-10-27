import { Outlet } from "react-router";
import { VersionBanner } from "./VersionBanner";

export function Root() {
  return (
    <>
      <VersionBanner />
      <Outlet />
    </>
  );
}
