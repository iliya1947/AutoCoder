import { useState } from "react";

function App() {
  const [count, setCount] = useState(0);

  return (
    <div style={{ fontFamily: "sans-serif", padding: "20px" }}>
      <h1>AutoCoder MVP</h1>
      <p>Добро пожаловать в рабочую среду AutoCoder!</p>
      <div>
        <button onClick={() => setCount((c) => c + 1)}>
          Нажато {count} раз
        </button>
      </div>
    </div>
  );
}

export default App;
