"use client";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useAuth } from "../auth/AuthContext";

const Context = createContext(null);
export function RealtimeProvider({ children }) {
  const { token, baseUrl } = useAuth();
  const [status, setStatus] = useState("disconnected");
  const [events, setEvents] = useState([]);
  const [generation, setGeneration] = useState(0);
  const retry = useRef(0);
  useEffect(() => {
    if (!token) return;
    let socket;
    let timer;
    let stopped = false;
    const connect = () => {
      setStatus(retry.current ? "reconnecting" : "connecting");
      const url = baseUrl.replace(/^http/, "ws") + "/events";
      socket = new WebSocket(url);
      socket.onopen = () => {
        setEvents([]);
        socket.send(JSON.stringify({ token }));
      };
      socket.onmessage = ({ data }) => {
        let message;
        try {
          message = JSON.parse(data);
        } catch {
          return;
        }
        if (message.type === "authenticated") {
          retry.current = 0;
          setStatus("connected");
          setGeneration((v) => v + 1);
          return;
        }
        if (message.type === "resync_required") {
          setGeneration((v) => v + 1);
        }
        setEvents((items) => [message, ...items].slice(0, 250));
      };
      socket.onerror = () => socket.close();
      socket.onclose = () => {
        if (stopped) return;
        setStatus("reconnecting");
        retry.current += 1;
        timer = setTimeout(
          connect,
          Math.min(30000, 1000 * 2 ** Math.min(retry.current, 5)),
        );
      };
    };
    connect();
    return () => {
      stopped = true;
      clearTimeout(timer);
      socket?.close();
    };
  }, [token, baseUrl]);
  const value = useMemo(
    () => ({
      status: token ? status : "disconnected",
      events: token ? events : [],
      generation,
      clear: () => setEvents([]),
    }),
    [token, status, events, generation],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export function useRealtime() {
  const value = useContext(Context);
  if (!value) throw new Error("RealtimeProvider required");
  return value;
}
