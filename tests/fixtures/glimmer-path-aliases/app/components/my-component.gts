import MyService from 'my-app/services/my-service';

export default class MyComponent {
  service = MyService;
}

<template>
  <div>{{this.service}}</div>
</template>
