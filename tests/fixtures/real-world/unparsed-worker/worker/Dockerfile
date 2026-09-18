FROM mcr.microsoft.com/dotnet/sdk:8.0
WORKDIR /source
COPY . .
RUN dotnet publish -o /app
ENTRYPOINT ["dotnet", "/app/Worker.dll"]
