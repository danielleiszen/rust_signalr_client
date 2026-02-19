using Microsoft.Extensions.Options;
using SignalRTestService;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddControllersWithViews();
builder.Services.AddSignalR().AddMessagePackProtocol();

builder.Services.AddCors(a =>
{ 
    //a.AddPolicy(name: "restricted",
    //    policy =>
    //    {
    //        policy.WithOrigins("http://example.com");
    //        policy.AllowAnyHeader();
    //        policy.WithMethods("GET", "POST");
    //        policy.AllowCredentials();
    //    });

    a.AddDefaultPolicy(builder =>
    {
        builder.AllowAnyOrigin()
        .AllowAnyHeader()        
        .AllowAnyMethod();
    });
});

var webSocketOptions = new WebSocketOptions
{
    KeepAliveInterval = TimeSpan.FromMinutes(2)
};

//webSocketOptions.AllowedOrigins.Add("http://localhost:8089");

var app = builder.Build();

app.UseCors();
app.UseWebSockets(webSocketOptions);

app.MapGet("/", () => "Hello World!");
app.MapHub<TestHub>("/test");

app.Run();
