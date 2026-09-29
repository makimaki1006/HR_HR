/** Isolated so tests can replace the hard navigation. */
export function redirectToLogin(): void {
  window.location.assign('/login');
}
