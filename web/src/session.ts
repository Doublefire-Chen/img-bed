import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, isUnauthorized, type SessionInfo } from "./api";

export const SESSION_KEY = ["session"] as const;

/** The Owner's session, or `null` when logged out. */
export function useSession() {
  return useQuery({
    queryKey: SESSION_KEY,
    queryFn: async (): Promise<SessionInfo | null> => {
      try {
        return await api.session();
      } catch (error) {
        if (isUnauthorized(error)) return null;
        throw error;
      }
    },
    staleTime: 60_000,
  });
}

export function useLogin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      username,
      password,
      rememberMe,
    }: {
      username: string;
      password: string;
      rememberMe: boolean;
    }) => api.login(username, password, rememberMe),
    onSuccess: (session) => queryClient.setQueryData(SESSION_KEY, session),
  });
}

export function useLogout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: api.logout,
    onSettled: () => {
      // Whatever the server said, this browser is logged out now.
      queryClient.setQueryData(SESSION_KEY, null);
      queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== SESSION_KEY[0] });
    },
  });
}
