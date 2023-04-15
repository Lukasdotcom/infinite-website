# Infinite Website
This is a website that has infinite pages by using AI to generate the webpages. To view a demo, go to [infinity.lschaefer.xyz](https://infinity.lschaefer.xyz). If you want to run it yourself you can follow the instructions below.
First get yourself a cookie from [here](https://open-assistant.io). Then run the following command: replacing the XXXXXXXXXX with your cookie.
```zsh
docker run -d -p 8080:8080 -e COOKIE=XXXXXXXXXX --name infinity --restart=always lukasdotcom/infinite-website
```