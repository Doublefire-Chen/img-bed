import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import { isUnauthorized } from "./api";
import { Layout } from "./components/Layout";
import { RequireOwner } from "./components/RequireOwner";
import { Root } from "./components/Root";
import { ImagePage } from "./pages/ImagePage";
import { LibraryPage } from "./pages/LibraryPage";
import { LoginPage } from "./pages/LoginPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TokensPage } from "./pages/TokensPage";
import { TrashPage } from "./pages/TrashPage";
import { UploadPage } from "./pages/UploadPage";
import { SESSION_KEY } from "./session";
import "./styles.css";

// Any 401 from the API means the session has ended (expired, logged out
// elsewhere, or the password was reset): drop to the login screen.
const onError = (error: unknown) => {
  if (isUnauthorized(error)) queryClient.setQueryData(SESSION_KEY, null);
};

const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError }),
  mutationCache: new MutationCache({ onError }),
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => !isUnauthorized(error) && failureCount < 2,
    },
  },
});

const router = createBrowserRouter(
  [
    {
      element: <Root />,
      children: [
        { path: "login", element: <LoginPage /> },
        {
          element: (
            <RequireOwner>
              <Layout />
            </RequireOwner>
          ),
          children: [
            { index: true, element: <UploadPage /> },
            { path: "library", element: <LibraryPage /> },
            { path: "library/:id", element: <ImagePage /> },
            { path: "trash", element: <TrashPage /> },
            { path: "tokens", element: <TokensPage /> },
            { path: "settings", element: <SettingsPage /> },
            { path: "*", element: <NotFoundPage /> },
          ],
        },
      ],
    },
  ],
  { basename: "/admin" },
);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
