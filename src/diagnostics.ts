import { invoke as tauriInvoke } from "@tauri-apps/api/core";
export type DiagnosticEvent = { timestamp:string; subsystem:string; component:string; eventType:string; severity:string; traceId:string; spanId:string; parentSpanId?:string; data:unknown; result?:unknown; error?:unknown; stateTransition?:unknown; durationMs?:number };
const sessionId=`session-${Date.now()}-${Math.random().toString(36).slice(2)}`; let sequence=0;
const id=(prefix:string)=>`${prefix}-${Date.now()}-${++sequence}`;
const componentFor=(command:string)=> command.includes("backup")?"backup-restore":command.includes("project_command")?"terminal-tool":command.includes("project_file")||command.includes("project_entry")?"file-tool":command.includes("chat")?"orchestration":command.includes("history")||command.includes("orchestration_task")?"persistence":command.includes("workspace")||command.includes("project")?"workspace":"tauri";
const safeShape=(value:unknown):unknown => value == null ? value : Array.isArray(value) ? {kind:"array",length:value.length} : typeof value === "object" ? {kind:"object",keys:Object.keys(value).slice(0,50)} : typeof value === "string" ? {kind:"string",length:value.length} : value;
export async function invoke<T>(command:string,args?:Record<string,unknown>):Promise<T>{
 if(command.startsWith("diagnostic_")||command==="record_diagnostic_event") return tauriInvoke<T>(command,args);
 const spanId=id("operation"),started=performance.now(),base={subsystem:"frontend",component:componentFor(command),traceId:sessionId,spanId,parentSpanId:sessionId,data:{command,arguments:safeShape(args)}};
 void tauriInvoke("record_diagnostic_event",{event:{...base,eventType:"boundary.requested",severity:"info"}}).catch(()=>undefined);
 try{const result=await tauriInvoke<T>(command,args);void tauriInvoke("record_diagnostic_event",{event:{...base,eventType:"boundary.completed",severity:"info",result:safeShape(result),durationMs:Math.round(performance.now()-started)}}).catch(()=>undefined);return result}catch(error){void tauriInvoke("record_diagnostic_event",{event:{...base,eventType:"boundary.rejected",severity:"error",error:{message:String(error)},durationMs:Math.round(performance.now()-started)}}).catch(()=>undefined);throw error}
}
