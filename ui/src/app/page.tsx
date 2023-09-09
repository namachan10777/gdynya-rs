import { css } from "../../styled-system/css";
import { center } from "../../styled-system/patterns/center";

export default function Home() {
  return (
    <main className={center()}>
      <h1 className={css({ fontSize: "xl" })}>Hello World!</h1>
    </main>
  );
}
