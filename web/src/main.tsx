import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import { isUnauthorized } from "./api";
import { Layout } from "./components/Layout";
import { RequireOwner } from "./components/RequireOwner";
import { Root } from "./components/Root";
import { HomePage } from "./pages/HomePage";
import { LoginPage } from "./pages/LoginPage";
import { NotFoundPage } from "./pages/NotFoundPage";
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
            { index: true, element: <HomePage /> },
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
