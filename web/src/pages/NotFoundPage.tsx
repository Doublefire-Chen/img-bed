import { Link } from "react-router";

export function NotFoundPage() {
  return (
    <section>
      <h1>Not found</h1>
      <p>
        There is no such page in the admin area. <Link to="/">Back to the Library</Link>.
      </p>
    </section>
  );
}
