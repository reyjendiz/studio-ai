' Starts the native Studio app
CreateObject("WScript.Shell").Run """" & CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName) & "\app\Studio.exe""", 1, False
