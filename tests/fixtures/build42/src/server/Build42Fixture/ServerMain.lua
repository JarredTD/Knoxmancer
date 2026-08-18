local ServerMain = {}

function ServerMain.onServerStarted()
    print("Build42Fixture server loaded")
end

Events.OnServerStarted.Add(ServerMain.onServerStarted)

return ServerMain
