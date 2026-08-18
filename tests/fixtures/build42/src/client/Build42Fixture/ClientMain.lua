local ClientMain = {}

function ClientMain.onGameStart()
    print("Build42Fixture client loaded")
end

Events.OnGameStart.Add(ClientMain.onGameStart)

return ClientMain
